#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const packageJson = read("apps/ui/package.json");
const packageLock = read("apps/ui/package-lock.json");
const adr = read("docs/adr/adr-008-supply-chain.md");
const doc = read("docs/security/g123-npm-advisory-scan.md");
const failures = [];
const npmAuditTimeoutMs = 120_000;

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requireText(name, text, token) {
  if (!text.includes(token)) failures.push(`${name} missing token: ${token}`);
}

for (const token of [
  "test:npm-audit-g123",
  "npm --prefix apps/ui audit --audit-level=high --omit=dev",
  "npm --prefix apps/ui audit --audit-level=high",
  "zero high-or-critical advisories",
  "There are no G123 npm\nadvisory waivers",
])
  requireText("docs/security/g123-npm-advisory-scan.md", doc, token);
for (const token of [
  "test:npm-audit-g123",
  "npm audit --audit-level=high",
  "documented non-release waiver",
])
  requireText("ADR-008", adr, token);
requireText("package.json", packageJson, "test:npm-audit-g123");
for (const token of [
  '"lockfileVersion"',
  '"packages"',
  '"node_modules/vite"',
]) {
  requireText("apps/ui/package-lock.json", packageLock, token);
}

function packageNameFromLockPath(lockPath) {
  const parts = lockPath.split("node_modules/");
  return parts[parts.length - 1];
}

function auditRequestBody({ omitDev }) {
  const lock = JSON.parse(packageLock);
  const body = {};
  for (const [lockPath, entry] of Object.entries(lock.packages ?? {})) {
    if (!lockPath || !entry?.version) continue;
    if (omitDev && entry.dev === true) continue;
    const name = entry.name ?? packageNameFromLockPath(lockPath);
    if (!name || name === lockPath) continue;
    body[name] ??= [];
    if (!body[name].includes(entry.version)) body[name].push(entry.version);
  }
  return body;
}

function postBulkAdvisories(body) {
  const payload = JSON.stringify(body);
  const result = spawnSync(
    "curl",
    [
      "-fsS",
      "--http1.1",
      "--retry",
      "2",
      "--max-time",
      "30",
      "-H",
      "content-type: application/json",
      "-H",
      "accept: application/json",
      "--data-binary",
      "@-",
      "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk",
    ],
    {
      cwd: repoRoot,
      input: payload,
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 16,
    },
  );
  if (result.status !== 0) {
    throw new Error(
      `bulk advisory endpoint failed:\n${result.stdout}\n${result.stderr}`.trim(),
    );
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(
      `bulk advisory endpoint emitted invalid JSON: ${error.message}`,
    );
  }
}

function osvQueries({ omitDev }) {
  return Object.entries(auditRequestBody({ omitDev })).flatMap(
    ([name, versions]) =>
      versions.map((version) => ({
        package: { name, ecosystem: "npm" },
        version,
      })),
  );
}

function requestOsvJson(url, body = null) {
  const args = [
    "-fsS",
    "--http1.1",
    "--retry",
    "2",
    "--retry-all-errors",
    "--max-time",
    "30",
    "-H",
    "accept: application/json",
  ];
  if (body !== null) {
    args.push("-H", "content-type: application/json", "--data-binary", "@-");
  }
  args.push(url);
  const result = spawnSync("curl", args, {
    cwd: repoRoot,
    input: body === null ? undefined : JSON.stringify(body),
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 16,
  });
  if (result.status !== 0) {
    throw new Error(
      `OSV.dev request failed for ${url}:\n${result.stdout}\n${result.stderr}`.trim(),
    );
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(
      `OSV.dev emitted invalid JSON for ${url}: ${error.message}`,
    );
  }
}

function cvssMetrics(vector) {
  return Object.fromEntries(
    String(vector)
      .split("/")
      .filter((part) => part.includes(":"))
      .map((part) => {
        const separator = part.indexOf(":");
        return [part.slice(0, separator), part.slice(separator + 1)];
      }),
  );
}

function roundCvssUp(value) {
  return Math.ceil((value - Number.EPSILON) * 10) / 10;
}

function cvssV3BaseScore(vector) {
  const metrics = cvssMetrics(vector);
  const scopeChanged = metrics.S === "C";
  const attackVector = { N: 0.85, A: 0.62, L: 0.55, P: 0.2 }[metrics.AV];
  const attackComplexity = { L: 0.77, H: 0.44 }[metrics.AC];
  const privilegesRequired = scopeChanged
    ? { N: 0.85, L: 0.68, H: 0.5 }[metrics.PR]
    : { N: 0.85, L: 0.62, H: 0.27 }[metrics.PR];
  const userInteraction = { N: 0.85, R: 0.62 }[metrics.UI];
  const confidentiality = { H: 0.56, L: 0.22, N: 0 }[metrics.C];
  const integrity = { H: 0.56, L: 0.22, N: 0 }[metrics.I];
  const availability = { H: 0.56, L: 0.22, N: 0 }[metrics.A];
  const values = [
    attackVector,
    attackComplexity,
    privilegesRequired,
    userInteraction,
    confidentiality,
    integrity,
    availability,
  ];
  if (values.some((value) => value === undefined)) return null;
  const impactBase =
    1 - (1 - confidentiality) * (1 - integrity) * (1 - availability);
  const impact = scopeChanged
    ? 7.52 * (impactBase - 0.029) - 3.25 * (impactBase - 0.02) ** 15
    : 6.42 * impactBase;
  if (impact <= 0) return 0;
  const exploitability =
    8.22 *
    attackVector *
    attackComplexity *
    privilegesRequired *
    userInteraction;
  const base = scopeChanged
    ? Math.min(1.08 * (impact + exploitability), 10)
    : Math.min(impact + exploitability, 10);
  return roundCvssUp(base);
}

function cvssV2BaseScore(vector) {
  const metrics = cvssMetrics(vector);
  const attackVector = { N: 1, A: 0.646, L: 0.395 }[metrics.AV];
  const attackComplexity = { L: 0.71, M: 0.61, H: 0.35 }[metrics.AC];
  const authentication = { N: 0.704, S: 0.56, M: 0.45 }[metrics.Au];
  const confidentiality = { C: 0.66, P: 0.275, N: 0 }[metrics.C];
  const integrity = { C: 0.66, P: 0.275, N: 0 }[metrics.I];
  const availability = { C: 0.66, P: 0.275, N: 0 }[metrics.A];
  const values = [
    attackVector,
    attackComplexity,
    authentication,
    confidentiality,
    integrity,
    availability,
  ];
  if (values.some((value) => value === undefined)) return null;
  const impact =
    10.41 * (1 - (1 - confidentiality) * (1 - integrity) * (1 - availability));
  if (impact <= 0) return 0;
  const exploitability = 20 * attackVector * attackComplexity * authentication;
  return (
    Math.round((0.6 * impact + 0.4 * exploitability - 1.5) * 1.176 * 10) / 10
  );
}

function officialSeverityScore(entry) {
  if (!entry || typeof entry !== "object") return null;
  if (entry.type === "CVSS_V3") return cvssV3BaseScore(entry.score);
  if (entry.type === "CVSS_V2") return cvssV2BaseScore(entry.score);
  if (entry.type === "Ubuntu") {
    const qualitative = String(entry.score ?? "").toLowerCase();
    if (["high", "critical"].includes(qualitative)) return 7;
    if (["low", "medium", "moderate", "negligible"].includes(qualitative)) {
      return 0;
    }
  }
  return null;
}

function runOsvAdvisoryFallback(label, options) {
  const queries = osvQueries(options);
  const results = [];
  for (let offset = 0; offset < queries.length; offset += 1000) {
    const queryBatch = queries.slice(offset, offset + 1000);
    let batch;
    try {
      batch = requestOsvJson("https://api.osv.dev/v1/querybatch", {
        queries: queryBatch,
      });
    } catch (error) {
      failures.push(`${label} OSV.dev fallback failed: ${error.message}`);
      return;
    }
    if (
      !Array.isArray(batch.results) ||
      batch.results.length !== queryBatch.length
    ) {
      failures.push(
        `${label} OSV.dev fallback returned ${Array.isArray(batch.results) ? batch.results.length : "no"} results for ${queryBatch.length} queries`,
      );
      return;
    }
    if (
      batch.results.some(
        (result) =>
          !result ||
          !isRecord(result) ||
          ("vulns" in result && !Array.isArray(result.vulns)) ||
          result.vulns?.some(
            (advisory) =>
              !isRecord(advisory) ||
              typeof advisory.id !== "string" ||
              advisory.id.trim().length === 0,
          ),
      )
    ) {
      failures.push(
        `${label} OSV.dev fallback returned a malformed batch result`,
      );
      return;
    }
    if (batch.results.some((result) => result?.next_page_token)) {
      failures.push(
        `${label} OSV.dev fallback returned a paginated batch result that requires review`,
      );
      return;
    }
    results.push(...batch.results);
  }

  const affectedPackagesById = new Map();
  for (const [index, result] of results.entries()) {
    const query = queries[index];
    for (const advisory of result?.vulns ?? []) {
      const affected = affectedPackagesById.get(advisory.id) ?? {
        labels: new Set(),
        names: new Set(),
      };
      affected.labels.add(`${query.package.name}@${query.version}`);
      affected.names.add(query.package.name);
      affectedPackagesById.set(advisory.id, affected);
    }
  }

  const severe = [];
  for (const [id, affectedPackages] of affectedPackagesById) {
    let advisory;
    try {
      advisory = requestOsvJson(
        `https://api.osv.dev/v1/vulns/${encodeURIComponent(id)}`,
      );
    } catch (error) {
      failures.push(
        `${label} OSV.dev advisory detail failed for ${id}: ${error.message}`,
      );
      continue;
    }
    if (
      !isRecord(advisory) ||
      advisory.id !== id ||
      (advisory.severity != null && !Array.isArray(advisory.severity)) ||
      !Array.isArray(advisory.affected) ||
      advisory.affected.some(
        (affected) =>
          !isRecord(affected) ||
          !isRecord(affected.package) ||
          typeof affected.package.name !== "string" ||
          (affected.severity != null && !Array.isArray(affected.severity)),
      ) ||
      (advisory.database_specific != null &&
        !isRecord(advisory.database_specific))
    ) {
      failures.push(`${label} OSV.dev advisory detail was malformed for ${id}`);
      continue;
    }
    const qualitativeSeverity = String(
      advisory.database_specific?.severity ?? "",
    ).toLowerCase();
    const officialSeverities = [
      ...(Array.isArray(advisory.severity) ? advisory.severity : []),
      ...(Array.isArray(advisory.affected)
        ? advisory.affected
            .filter((affected) =>
              affectedPackages.names.has(affected?.package?.name),
            )
            .flatMap((affected) =>
              Array.isArray(affected.severity) ? affected.severity : [],
            )
        : []),
    ];
    const officialScores = officialSeverities.map(officialSeverityScore);
    const hasUnknownOfficialSeverity = officialScores.some(
      (score) => score === null,
    );
    const hasSevereOfficialScore = officialScores.some(
      (score) => score !== null && score >= 7,
    );
    if (
      ["high", "critical"].includes(qualitativeSeverity) ||
      hasSevereOfficialScore
    ) {
      severe.push(
        `${[...affectedPackages.labels].join(", ")}: high/critical ${advisory.summary ?? advisory.id}`,
      );
    } else if (
      hasUnknownOfficialSeverity ||
      (officialScores.length === 0 &&
        !["info", "low", "medium", "moderate", "negligible"].includes(
          qualitativeSeverity,
        ))
    ) {
      severe.push(
        `${[...affectedPackages.labels].join(", ")}: unclassified ${advisory.id} requires fail-closed review`,
      );
    }
  }
  if (severe.length > 0) {
    failures.push(
      `${label} OSV.dev fallback reported blocking advisories:\n${severe.join("\n")}`,
    );
  }
}

function isEndpointUnavailable(result) {
  const diagnostic = [
    result.stdout,
    result.stderr,
    result.error?.code,
    result.error?.message,
  ]
    .filter(Boolean)
    .join("\n");
  return /audit endpoint returned an error|ETIMEDOUT|ECONNRESET|ENOTFOUND|EAI_AGAIN|FetchError|Service Unavailable/i.test(
    diagnostic,
  );
}

function runBulkAdvisoryFallback(label, { omitDev }) {
  let advisories;
  try {
    advisories = postBulkAdvisories(auditRequestBody({ omitDev }));
  } catch (error) {
    console.warn(
      `${label}: npm bulk advisory endpoint unavailable; using independent OSV.dev fallback`,
    );
    runOsvAdvisoryFallback(label, { omitDev });
    return;
  }
  if (
    !isRecord(advisories) ||
    Object.values(advisories).some(
      (entries) =>
        !Array.isArray(entries) ||
        entries.some(
          (advisory) =>
            !isRecord(advisory) ||
            typeof advisory.severity !== "string" ||
            !["info", "low", "moderate", "high", "critical"].includes(
              advisory.severity.toLowerCase(),
            ),
        ),
    )
  ) {
    failures.push(`${label} bulk advisory fallback returned malformed data`);
    return;
  }
  const severe = [];
  for (const [name, entries] of Object.entries(advisories ?? {})) {
    for (const advisory of entries ?? []) {
      if (["high", "critical"].includes(advisory.severity.toLowerCase())) {
        severe.push(
          `${name}: ${advisory.severity} ${advisory.title ?? advisory.url ?? advisory.id ?? "advisory"}`,
        );
      }
    }
  }
  if (severe.length > 0)
    failures.push(
      `${label} bulk advisory fallback reported high/critical advisories:\n${severe.join("\n")}`,
    );
}

function runAudit(label, args, options) {
  const result = spawnSync("npm", args, {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 16,
    timeout: npmAuditTimeoutMs,
  });
  if (result.status !== 0) {
    if (isEndpointUnavailable(result)) {
      console.warn(
        `${label}: npm audit endpoint unavailable; using npm bulk advisory endpoint fallback`,
      );
      runBulkAdvisoryFallback(label, options);
      return;
    }
    failures.push(
      `${label} failed:\n${result.stdout}\n${result.stderr}`.trim(),
    );
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (error) {
    failures.push(`${label} did not emit parseable JSON: ${error.message}`);
    return;
  }
  const counts = parsed?.metadata?.vulnerabilities;
  const high = counts?.high;
  const critical = counts?.critical;
  if (
    !isRecord(parsed) ||
    !isRecord(counts) ||
    !Number.isFinite(high) ||
    high < 0 ||
    !Number.isFinite(critical) ||
    critical < 0
  ) {
    failures.push(`${label} emitted a malformed vulnerability summary`);
    return;
  }
  if (high > 0 || critical > 0) {
    failures.push(`${label} reported high=${high}, critical=${critical}`);
  }
}

runAudit(
  "npm production audit",
  [
    "--prefix",
    "apps/ui",
    "audit",
    "--audit-level=high",
    "--omit=dev",
    "--json",
  ],
  { omitDev: true },
);
runAudit(
  "npm full UI audit",
  ["--prefix", "apps/ui", "audit", "--audit-level=high", "--json"],
  { omitDev: false },
);

if (failures.length > 0) {
  console.error("G123 npm-audit gate failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("G123 npm-audit gate passed: zero high-or-critical advisories");
