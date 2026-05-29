import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  discryptUiConfig,
  setupChecklist,
  ThemeId,
  TemplateId,
} from "./app-config";
import {
  AppMessageView,
  AppSnapshot,
  AppState,
  ChannelKind,
  ChannelStateView,
  DirectConversationView,
  GroupView,
  VoiceParticipantView,
  createChannel as createChannelCommand,
  createGroup,
  createInvite,
  createUser,
  joinGroup,
  joinVoice,
  leaveVoice,
  loadAppState,
  recoverUser,
  savePreferences,
  sendMessage,
  setSelfMute,
  setSpeakerVolume,
  startDm,
  verifySafetyNumber,
} from "./commands";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import "./styles.css";

type Workflow = "setup" | "dm" | "join" | "create-group" | "channel" | "voice";
type SetupStepView = { label: string; complete: boolean; detail: string };
type VoiceParticipant = VoiceParticipantView;

function asThemeId(value: string): ThemeId {
  return discryptUiConfig.themes.some((theme) => theme.id === value)
    ? (value as ThemeId)
    : discryptUiConfig.activeTheme;
}

function asTemplateId(value: string): TemplateId {
  return discryptUiConfig.templates.some((template) => template.id === value)
    ? (value as TemplateId)
    : discryptUiConfig.activeTemplate;
}

function Icon({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex h-4 w-4 items-center justify-center leading-none",
        className,
      )}
    >
      {children}
    </span>
  );
}

function App() {
  const [commandState, setCommandState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [workflow, setWorkflow] = useState<Workflow>("setup");
  const [draftChannel, setDraftChannel] = useState("general");
  const [draftMessage, setDraftMessage] = useState(
    "Hello from the command-backed UI",
  );
  const [draftGroup, setDraftGroup] = useState("private lab");
  const [draftInvite, setDraftInvite] = useState("invite:joined-enclave");
  const [draftJoinName, setDraftJoinName] = useState("joined enclave");
  const [draftDisplayName, setDraftDisplayName] = useState("Alice");
  const [draftDeviceName, setDraftDeviceName] = useState("Desktop");
  const [draftRecoveryCode, setDraftRecoveryCode] = useState(
    "local recovery placeholder",
  );
  const [draftDmName, setDraftDmName] = useState("Bob");
  const [inspectorOpen, setInspectorOpen] = useState(false);

  useEffect(() => {
    let mounted = true;
    loadAppState()
      .then((loaded) => mounted && setCommandState(loaded))
      .catch(
        (error: unknown) =>
          mounted &&
          setLoadError(
            error instanceof Error
              ? error.message
              : "Unable to load app command state",
          ),
      );
    return () => {
      mounted = false;
    };
  }, []);

  async function applyCommand(
    command: Promise<AppState>,
    success?: (state: AppState) => void,
  ) {
    try {
      setCommandError(null);
      const nextState = await command;
      setCommandState(nextState);
      success?.(nextState);
    } catch (error: unknown) {
      setCommandError(
        error instanceof Error ? error.message : "Command failed",
      );
    }
  }

  if (loadError) {
    return (
      <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-red-200">
        discrypt command surface failed: {loadError}
      </main>
    );
  }
  if (!commandState) {
    return (
      <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">
        Loading discrypt…
      </main>
    );
  }

  const appState = commandState;
  const currentSnapshot = appState.snapshot;
  const activeGroup = getActiveGroup(appState);
  const activeTextChannel = getActiveTextChannel(appState, activeGroup);
  const activeVoiceChannel = getActiveVoiceChannel(appState, activeGroup);
  const textChannels =
    activeGroup?.channels.filter((channel) => channel.kind === "Text") ?? [];
  const voiceChannels =
    activeGroup?.channels.filter((channel) => channel.kind === "Voice") ?? [];
  const activeDm = getActiveDm(appState);
  const groupLabel = activeGroup?.name ?? "Local profile";
  const participants = appState.voice_session?.participants ?? [];
  const voiceJoined = appState.voice_session?.joined ?? false;
  const selfMuted =
    appState.voice_session?.self_muted ??
    participants.find((participant) => participant.id === "local-user")
      ?.muted ??
    false;
  const activeTheme =
    discryptUiConfig.themes.find(
      (theme) => theme.id === appState.preferences.theme_id,
    ) ?? discryptUiConfig.themes[0];
  const activeTemplate =
    discryptUiConfig.templates.find(
      (template) => template.id === appState.preferences.template_id,
    ) ?? discryptUiConfig.templates[0];
  const themeStyle = activeTheme.cssVars as React.CSSProperties;
  const setupSteps: SetupStepView[] = [
    {
      label: setupChecklist[0],
      complete: currentSnapshot.friend.verified,
      detail: currentSnapshot.friend.verified
        ? "Safety number verified"
        : "Compare the number before trusting the DM",
    },
    {
      label: setupChecklist[1],
      complete: appState.devices.length >= 1,
      detail: `${appState.devices.length} authorized local device${appState.devices.length === 1 ? "" : "s"}`,
    },
    {
      label: setupChecklist[2],
      complete: currentSnapshot.invite.welcome_required.length > 0,
      detail: "Invite admission copy is present",
    },
    {
      label: setupChecklist[3],
      complete: currentSnapshot.retention.selected.length > 0,
      detail: `Retention preset: ${currentSnapshot.retention.selected}`,
    },
  ];
  const completedSteps = setupSteps.filter((step) => step.complete).length;
  const showInspector =
    activeTemplate.showRightRail && inspectorOpen && workflow !== "setup";

  async function confirmSafetyNumber() {
    try {
      const result = await verifySafetyNumber({
        friend_id: currentSnapshot.friend.friend_code,
        provided: currentSnapshot.friend.safety_number,
      });
      setVerifyMessage(result.message);
      if (result.verified) await applyCommand(loadAppState());
    } catch (error: unknown) {
      setVerifyMessage(
        `Safety verification command failed: ${error instanceof Error ? error.message : "unknown error"}`,
      );
    }
  }

  function createCommandUser() {
    void applyCommand(
      createUser({
        display_name: draftDisplayName,
        device_name: draftDeviceName,
      }),
      () => setWorkflow("setup"),
    );
  }

  function recoverCommandUser() {
    void applyCommand(
      recoverUser({
        display_name: draftDisplayName,
        device_name: draftDeviceName,
        recovery_code: draftRecoveryCode,
      }),
      () => setWorkflow("setup"),
    );
  }

  function createCommandGroup() {
    void applyCommand(
      createGroup({
        name: draftGroup,
        retention: currentSnapshot.retention.selected,
      }),
      (state) => {
        const group = getActiveGroup(state);
        setDraftGroup(group?.name ?? draftGroup);
        setWorkflow("channel");
      },
    );
  }

  function joinCommandGroup() {
    void applyCommand(
      joinGroup({
        invite_code: draftInvite,
        group_name: draftJoinName || null,
      }),
      (state) => {
        const group = getActiveGroup(state);
        setDraftJoinName(group?.name ?? draftJoinName);
        setWorkflow("channel");
      },
    );
  }

  function startCommandDm() {
    void applyCommand(startDm({ display_name: draftDmName }), () =>
      setWorkflow("dm"),
    );
  }

  function createCommandChannel(kind: ChannelKind = "Text") {
    if (!activeGroup) {
      setCommandError("Create or join a group before adding a channel.");
      return;
    }
    const name =
      draftChannel.trim().replace(/^#/, "") ||
      (kind === "Text" ? "general" : "Voice Lobby");
    void applyCommand(
      createChannelCommand({
        group_id: activeGroup.group_id,
        name,
        kind,
        retention_status:
          kind === "Voice" ? "session" : currentSnapshot.retention.selected,
      }),
      () => setWorkflow(kind === "Voice" ? "voice" : "channel"),
    );
  }

  function sendCommandMessage() {
    const body = draftMessage.trim();
    if (!body) return;
    if (!activeGroup || !activeTextChannel) {
      setCommandError("Create a group text channel before sending a message.");
      return;
    }
    void applyCommand(
      sendMessage({
        target: {
          kind: "channel",
          dm_id: null,
          group_id: activeGroup.group_id,
          channel_id: activeTextChannel.channel_id,
        },
        body,
      }),
      () => setDraftMessage(""),
    );
  }

  function sendCommandDm() {
    const body = draftMessage.trim();
    if (!body || !activeDm) return;
    void applyCommand(
      sendMessage({
        target: {
          kind: "dm",
          dm_id: activeDm.dm_id,
          group_id: null,
          channel_id: null,
        },
        body,
      }),
      () => setDraftMessage(""),
    );
  }

  function createCommandInvite() {
    if (!activeGroup) {
      setCommandError("Create or join a group before creating an invite.");
      return;
    }
    void applyCommand(
      createInvite({
        group_id: activeGroup.group_id,
        expires: currentSnapshot.invite.expires,
        max_use: currentSnapshot.invite.max_use,
      }),
      (state) => {
        const invite = state.invites.at(-1);
        if (invite) setDraftInvite(invite.code);
        setWorkflow("join");
      },
    );
  }

  function setVolume(id: string, value: number[]) {
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) {
      setCommandError("Join a voice channel before changing volume.");
      return;
    }
    void applyCommand(
      setSpeakerVolume({
        session_id: sessionId,
        participant_id: id,
        volume: value[0] ?? 0,
      }),
    );
  }

  function toggleSelfMute(checked: boolean) {
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) {
      setCommandError("Join a voice channel before muting.");
      return;
    }
    void applyCommand(setSelfMute({ session_id: sessionId, muted: checked }));
  }

  async function toggleVoiceJoin(joined: boolean) {
    if (joined) {
      if (!activeGroup) {
        setCommandError("Create or join a group before joining voice.");
        return;
      }
      let voiceChannel = activeVoiceChannel;
      if (!voiceChannel) {
        const withVoice = await createChannelCommand({
          group_id: activeGroup.group_id,
          name: "Voice Lobby",
          kind: "Voice",
          retention_status: "session",
        });
        setCommandState(withVoice);
        voiceChannel = getActiveVoiceChannel(
          withVoice,
          withVoice.groups.find(
            (group) => group.group_id === activeGroup.group_id,
          ) ?? null,
        );
      }
      if (!voiceChannel) {
        setCommandError("Voice channel creation did not return a channel.");
        return;
      }
      void applyCommand(
        joinVoice({
          group_id: activeGroup.group_id,
          channel_id: voiceChannel.channel_id,
        }),
        () => setWorkflow("voice"),
      );
      return;
    }
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) return;
    void applyCommand(leaveVoice({ session_id: sessionId }), () =>
      setWorkflow("voice"),
    );
  }

  function chooseTheme(nextTheme: ThemeId) {
    void applyCommand(
      savePreferences({ theme_id: nextTheme, template_id: activeTemplate.id }),
    );
  }

  function chooseTemplate(nextTemplate: TemplateId) {
    void applyCommand(
      savePreferences({ theme_id: activeTheme.id, template_id: nextTemplate }),
    );
  }

  if (appState.lifecycle === "first_run") {
    return (
      <FirstRunPanel
        themeStyle={themeStyle}
        displayName={draftDisplayName}
        setDisplayName={setDraftDisplayName}
        deviceName={draftDeviceName}
        setDeviceName={setDraftDeviceName}
        recoveryCode={draftRecoveryCode}
        setRecoveryCode={setDraftRecoveryCode}
        commandError={commandError}
        onCreate={createCommandUser}
        onRecover={recoverCommandUser}
      />
    );
  }

  return (
    <main
      data-template={activeTemplate.id}
      style={themeStyle}
      className={cn(
        "grid min-h-dvh overflow-hidden bg-[hsl(var(--background))] text-[hsl(var(--foreground))]",
        showInspector
          ? "grid-cols-1 lg:grid-cols-[72px_300px_minmax(0,1fr)] 2xl:grid-cols-[72px_300px_minmax(0,1fr)_330px]"
          : "grid-cols-1 lg:grid-cols-[72px_300px_minmax(0,1fr)]",
      )}
    >
      <ServerRail
        groups={appState.groups}
        activeGroup={activeGroup}
        themeLabel={activeTheme.label}
      />
      <ChannelSidebar
        groupLabel={groupLabel}
        role={activeGroup?.role ?? "local profile"}
        textChannels={textChannels}
        voiceChannels={voiceChannels}
        selectedWorkflow={workflow}
        onSelectWorkflow={setWorkflow}
        onOpenCreateGroup={() => setWorkflow("create-group")}
        onOpenJoin={() => setWorkflow("join")}
        onOpenChannel={() => setWorkflow("channel")}
        onOpenDm={() => setWorkflow("dm")}
        voiceJoined={voiceJoined}
        participants={participants}
        setupSteps={setupSteps}
        completedSteps={completedSteps}
      />
      <section className="flex h-dvh min-w-0 flex-col bg-[radial-gradient(circle_at_80%_0%,hsl(var(--primary)/0.10),transparent_34rem)]">
        <TopBar
          groupLabel={groupLabel}
          themeId={asThemeId(activeTheme.id)}
          templateId={asTemplateId(activeTemplate.id)}
          onThemeChange={chooseTheme}
          onTemplateChange={chooseTemplate}
          onOpenCreateGroup={() => setWorkflow("create-group")}
          onOpenJoin={() => setWorkflow("join")}
          onCreateInvite={createCommandInvite}
          onToggleInspector={() => setInspectorOpen((open) => !open)}
          inspectorOpen={inspectorOpen}
          canCreateInvite={Boolean(activeGroup)}
        />
        {commandError ? (
          <p className="mx-4 mt-3 rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100 md:mx-6">
            Command note: {commandError}
          </p>
        ) : null}
        {appState.invites.at(-1) ? (
          <p className="mx-4 mt-3 rounded-xl border border-emerald-300/30 bg-emerald-300/10 p-3 text-sm text-emerald-100 md:mx-6">
            Invite ready: {appState.invites.at(-1)?.code}
          </p>
        ) : null}
        <WorkflowNav workflow={workflow} setWorkflow={setWorkflow} />
        <ScrollArea className="min-h-0 flex-1 px-4 pb-4 md:px-6 md:pb-6">
          {workflow === "setup" ? (
            <SetupPanel
              snapshot={currentSnapshot}
              setupSteps={setupSteps}
              completedSteps={completedSteps}
              verifyMessage={verifyMessage}
              onVerify={confirmSafetyNumber}
            />
          ) : null}
          {workflow === "dm" ? (
            <DmPanel
              activeDm={activeDm}
              messages={appState.messages}
              draftDmName={draftDmName}
              setDraftDmName={setDraftDmName}
              draftMessage={draftMessage}
              setDraftMessage={setDraftMessage}
              onStartDm={startCommandDm}
              onSendDm={sendCommandDm}
            />
          ) : null}
          {workflow === "join" ? (
            <JoinPanel
              snapshot={currentSnapshot}
              inviteValue={draftInvite}
              setInviteValue={setDraftInvite}
              groupName={draftJoinName}
              setGroupName={setDraftJoinName}
              latestInvite={appState.invites.at(-1)?.code ?? null}
              onJoin={joinCommandGroup}
              onCreateInvite={createCommandInvite}
              canCreateInvite={Boolean(activeGroup)}
            />
          ) : null}
          {workflow === "create-group" ? (
            <CreateGroupPanel
              snapshot={currentSnapshot}
              groupName={draftGroup}
              setGroupName={setDraftGroup}
              onCreate={createCommandGroup}
            />
          ) : null}
          {workflow === "channel" ? (
            <ChannelPanel
              group={activeGroup}
              activeChannel={activeTextChannel}
              channels={textChannels}
              messages={appState.messages}
              draftChannel={draftChannel}
              setDraftChannel={setDraftChannel}
              draftMessage={draftMessage}
              setDraftMessage={setDraftMessage}
              onCreateTextChannel={() => createCommandChannel("Text")}
              onCreateVoiceChannel={() => createCommandChannel("Voice")}
              onSendMessage={sendCommandMessage}
            />
          ) : null}
          {workflow === "voice" ? (
            <VoicePanel
              group={activeGroup}
              activeVoiceChannel={activeVoiceChannel}
              route={
                appState.voice_session?.route_copy ??
                currentSnapshot.voice.route
              }
              participants={participants}
              voiceJoined={voiceJoined}
              selfMuted={selfMuted}
              setVoiceJoined={toggleVoiceJoin}
              setSelfMuted={toggleSelfMute}
              setVolume={setVolume}
            />
          ) : null}
        </ScrollArea>
      </section>
      {showInspector ? (
        <InspectorRail
          snapshot={currentSnapshot}
          appState={appState}
          participants={participants}
          completedSteps={completedSteps}
          themeLabel={activeTheme.label}
          templateLabel={activeTemplate.label}
        />
      ) : null}
    </main>
  );
}

function getActiveGroup(state: AppState): GroupView | null {
  const activeId = state.active_context?.group_id;
  if (activeId)
    return (
      state.groups.find((group) => group.group_id === activeId) ??
      state.groups[0] ??
      null
    );
  return state.groups[0] ?? null;
}

function getActiveTextChannel(
  state: AppState,
  group: GroupView | null,
): ChannelStateView | null {
  if (!group) return null;
  const activeId =
    state.active_context?.kind === "text_channel"
      ? state.active_context.channel_id
      : null;
  return (
    (activeId
      ? group.channels.find(
          (channel) =>
            channel.channel_id === activeId && channel.kind === "Text",
        )
      : null) ??
    group.channels.find((channel) => channel.kind === "Text") ??
    null
  );
}

function getActiveVoiceChannel(
  state: AppState,
  group: GroupView | null,
): ChannelStateView | null {
  if (!group) return null;
  const activeId =
    state.active_context?.kind === "voice_channel"
      ? state.active_context.channel_id
      : null;
  return (
    (activeId
      ? group.channels.find(
          (channel) =>
            channel.channel_id === activeId && channel.kind === "Voice",
        )
      : null) ??
    group.channels.find((channel) => channel.kind === "Voice") ??
    null
  );
}

function getActiveDm(state: AppState): DirectConversationView | null {
  const activeDmId = state.active_context?.dm_id ?? state.dms[0]?.dm_id ?? null;
  return activeDmId
    ? (state.dms.find((dm) => dm.dm_id === activeDmId) ?? state.dms[0] ?? null)
    : (state.dms[0] ?? null);
}

function FirstRunPanel({
  themeStyle,
  displayName,
  setDisplayName,
  deviceName,
  setDeviceName,
  recoveryCode,
  setRecoveryCode,
  commandError,
  onCreate,
  onRecover,
}: {
  themeStyle: React.CSSProperties;
  displayName: string;
  setDisplayName: (value: string) => void;
  deviceName: string;
  setDeviceName: (value: string) => void;
  recoveryCode: string;
  setRecoveryCode: (value: string) => void;
  commandError: string | null;
  onCreate: () => void;
  onRecover: () => void;
}) {
  return (
    <main
      style={themeStyle}
      className="min-h-dvh bg-[radial-gradient(circle_at_20%_10%,hsl(var(--primary)/0.12),transparent_24rem),hsl(var(--background))] p-4 text-[hsl(var(--foreground))] md:p-8"
    >
      <div className="mx-auto grid min-h-[calc(100dvh-2rem)] w-full max-w-5xl place-items-center md:min-h-[calc(100dvh-4rem)]">
        <Card className="w-full overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.9)] shadow-2xl shadow-black/30">
          <div className="grid lg:grid-cols-[0.9fr_1.1fr]">
            <CardHeader className="border-b border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.48),transparent)] p-6 lg:border-b-0 lg:border-r lg:p-8">
              <Badge variant="secondary" className="w-fit">
                first run
              </Badge>
              <CardTitle className="max-w-md text-3xl leading-tight md:text-4xl">
                Set up your local discrypt profile
              </CardTitle>
              <CardDescription className="max-w-md text-base leading-7">
                Create a local identity for this device, or unlock a test-build
                recovery placeholder. No cloud backup, history restore, QR
                pairing, or cross-device key recovery is claimed here.
              </CardDescription>
              <div className="grid gap-3 pt-3 text-sm text-[hsl(var(--muted-foreground))]">
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">
                  1. Choose a display name and device label.
                </div>
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">
                  2. Enter the app shell with command-backed local state.
                </div>
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">
                  3. Verify safety, groups, chat, and voice from the setup
                  checklist.
                </div>
              </div>
            </CardHeader>
            <CardContent className="grid gap-4 p-6 md:grid-cols-2 lg:p-8">
              {commandError ? (
                <p className="rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100 md:col-span-2">
                  Command note: {commandError}
                </p>
              ) : null}
              <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
                <div className="mb-4">
                  <h2 className="text-lg font-semibold">New local user</h2>
                  <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Best for first machine setup.
                  </p>
                </div>
                <Label className="grid gap-2">
                  Display name
                  <Input
                    value={displayName}
                    onChange={(event) => setDisplayName(event.target.value)}
                  />
                </Label>
                <Label className="mt-4 grid gap-2">
                  Device name
                  <Input
                    value={deviceName}
                    onChange={(event) => setDeviceName(event.target.value)}
                  />
                </Label>
                <Button className="mt-auto w-full" onClick={onCreate}>
                  Create new user
                </Button>
              </div>
              <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
                <div className="mb-4">
                  <h2 className="text-lg font-semibold">Existing user</h2>
                  <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                    Placeholder recovery for this local build.
                  </p>
                </div>
                <Label className="grid gap-2">
                  Recovery phrase/code
                  <Input
                    value={recoveryCode}
                    onChange={(event) => setRecoveryCode(event.target.value)}
                  />
                </Label>
                <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                  Local/test-build placeholder only. It unlocks the shell for
                  E2E coverage but does not recover remote devices or message
                  history.
                </p>
                <Button
                  variant="outline"
                  className="mt-auto w-full"
                  onClick={onRecover}
                >
                  Recover existing user
                </Button>
              </div>
            </CardContent>
          </div>
        </Card>
      </div>
    </main>
  );
}

function ServerRail({
  groups,
  activeGroup,
  themeLabel,
}: {
  groups: GroupView[];
  activeGroup: GroupView | null;
  themeLabel: string;
}) {
  return (
    <aside className="hidden border-r border-[hsl(var(--border))] bg-black/20 p-3 md:flex md:flex-col md:items-center md:gap-3">
      <div className="grid h-10 w-10 place-items-center rounded-2xl bg-[hsl(var(--primary))] font-black text-[hsl(var(--primary-foreground))] shadow-sm">
        d
      </div>
      {(groups.length
        ? groups
        : [
            {
              group_id: "local",
              name: "Local",
              role: "local profile",
              channels: [],
            },
          ]
      )
        .slice(0, 6)
        .map((group) => (
          <div
            key={group.group_id}
            title={group.name}
            className={cn(
              "grid h-11 w-11 place-items-center rounded-2xl border text-xs font-bold",
              group.group_id === activeGroup?.group_id
                ? "border-[hsl(var(--primary)/0.6)] bg-[hsl(var(--secondary))] text-[hsl(var(--foreground))]"
                : "border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]",
            )}
          >
            {group.name.slice(0, 2).toUpperCase()}
          </div>
        ))}
      <div
        className="mt-auto grid h-10 w-10 place-items-center rounded-xl border border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]"
        title={themeLabel}
      >
        cfg
      </div>
    </aside>
  );
}

function ChannelSidebar({
  groupLabel,
  role,
  textChannels,
  voiceChannels,
  selectedWorkflow,
  onSelectWorkflow,
  onOpenCreateGroup,
  onOpenJoin,
  onOpenChannel,
  onOpenDm,
  voiceJoined,
  participants,
  setupSteps,
  completedSteps,
}: {
  groupLabel: string;
  role: string;
  textChannels: ChannelStateView[];
  voiceChannels: ChannelStateView[];
  selectedWorkflow: Workflow;
  onSelectWorkflow: (workflow: Workflow) => void;
  onOpenCreateGroup: () => void;
  onOpenJoin: () => void;
  onOpenChannel: () => void;
  onOpenDm: () => void;
  voiceJoined: boolean;
  participants: VoiceParticipant[];
  setupSteps: SetupStepView[];
  completedSteps: number;
}) {
  const setupTotal = setupSteps.length;
  const setupProgress =
    setupTotal > 0 ? (completedSteps / setupTotal) * 100 : 0;
  const speaking = participants.filter(
    (participant) => participant.speaking && !participant.muted,
  ).length;
  return (
    <aside className="hidden h-dvh border-r border-[hsl(var(--border))] bg-[hsl(var(--card)/0.62)] backdrop-blur-xl lg:block">
      <div className="flex h-full flex-col">
        <div className="border-b border-[hsl(var(--border))] p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h1 className="text-lg font-semibold tracking-tight">
                {groupLabel}
              </h1>
              <p className="text-xs text-[hsl(var(--muted-foreground))]">
                {role} · command-backed state
              </p>
            </div>
            <Badge variant={voiceJoined ? "success" : "secondary"}>
              {voiceJoined ? "voice" : "ready"}
            </Badge>
          </div>
          <div className="mt-4 grid grid-cols-2 gap-2">
            <Button variant="secondary" size="sm" onClick={onOpenCreateGroup}>
              <Icon>+</Icon>Create
            </Button>
            <Button variant="outline" size="sm" onClick={onOpenJoin}>
              Join
            </Button>
          </div>
        </div>
        <ScrollArea className="min-h-0 flex-1 p-3">
          <Card className="mb-5 bg-[hsl(var(--secondary)/0.34)] shadow-none">
            <CardHeader className="p-4 pb-2">
              <div className="flex items-center justify-between">
                <CardTitle>Setup</CardTitle>
                <Badge variant="secondary">
                  {completedSteps} of {setupTotal}
                </Badge>
              </div>
              <div className="mt-2 h-1.5 rounded-full bg-[hsl(var(--muted))]">
                <div
                  className="h-full rounded-full bg-[hsl(var(--primary))]"
                  style={{ width: `${setupProgress}%` }}
                />
              </div>
            </CardHeader>
            <CardContent className="p-3 pt-1">
              <SidebarButton
                active={selectedWorkflow === "setup"}
                onClick={() => onSelectWorkflow("setup")}
                meta="trust checklist"
              >
                Setup checklist
              </SidebarButton>
            </CardContent>
          </Card>
          <SidebarButton
            active={selectedWorkflow === "dm"}
            onClick={onOpenDm}
            meta="direct conversation"
          >
            Direct messages
          </SidebarButton>
          <SectionLabel>Text channels</SectionLabel>
          {textChannels.length === 0 ? (
            <p className="px-2 text-xs text-[hsl(var(--muted-foreground))]">
              No text channel yet.
            </p>
          ) : null}
          {textChannels.map((channel) => (
            <SidebarButton
              key={channel.channel_id}
              active={selectedWorkflow === "channel"}
              onClick={onOpenChannel}
              meta={channel.retention_status}
            >
              {channel.name}
            </SidebarButton>
          ))}
          <Button
            variant="ghost"
            size="sm"
            className="mt-1 w-full justify-start"
            onClick={onOpenChannel}
          >
            <Icon>+</Icon>Create channel
          </Button>
          <SectionLabel>Voice rooms</SectionLabel>
          {voiceChannels.length === 0 ? (
            <p className="px-2 text-xs text-[hsl(var(--muted-foreground))]">
              No voice room yet.
            </p>
          ) : null}
          {voiceChannels.map((channel) => (
            <SidebarButton
              key={channel.channel_id}
              active={selectedWorkflow === "voice"}
              onClick={() => onSelectWorkflow("voice")}
              meta={voiceJoined ? `${speaking} speaking` : "not joined"}
            >
              {channel.name}
            </SidebarButton>
          ))}
        </ScrollArea>
      </div>
    </aside>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <p className="mb-2 mt-5 px-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
      {children}
    </p>
  );
}
function SidebarButton({
  children,
  active,
  meta,
  onClick,
}: {
  children: React.ReactNode;
  active?: boolean;
  meta?: string;
  onClick?: () => void;
}) {
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      className={cn(
        "mb-1 h-auto w-full justify-start whitespace-normal rounded-xl px-3 py-2 text-left text-sm text-[hsl(var(--muted-foreground))]",
        active && "bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]",
      )}
    >
      <span className="grid gap-0.5">
        <span className="font-medium">{children}</span>
        {meta ? (
          <span className="truncate text-[11px] text-[hsl(var(--muted-foreground))]">
            {meta}
          </span>
        ) : null}
      </span>
    </Button>
  );
}

function TopBar({
  groupLabel,
  themeId,
  templateId,
  onThemeChange,
  onTemplateChange,
  onOpenCreateGroup,
  onOpenJoin,
  onCreateInvite,
  onToggleInspector,
  inspectorOpen,
  canCreateInvite,
}: {
  groupLabel: string;
  themeId: ThemeId;
  templateId: TemplateId;
  onThemeChange: (id: ThemeId) => void;
  onTemplateChange: (id: TemplateId) => void;
  onOpenCreateGroup: () => void;
  onOpenJoin: () => void;
  onCreateInvite: () => void;
  onToggleInspector: () => void;
  inspectorOpen: boolean;
  canCreateInvite: boolean;
}) {
  return (
    <div className="border-b border-[hsl(var(--border))] bg-[hsl(var(--background)/0.82)] p-4 backdrop-blur-xl md:p-6">
      <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        <div className="min-w-0">
          <h2 className="truncate text-xl font-semibold tracking-tight">
            {groupLabel}
          </h2>
          <p className="text-xs text-[hsl(var(--muted-foreground))]">
            Local-first workspace · persisted through the Tauri command service
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" size="sm" onClick={onOpenCreateGroup}>
            <Icon>+</Icon>Create group
          </Button>
          <Button variant="outline" size="sm" onClick={onOpenJoin}>
            Join group
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={onCreateInvite}
            disabled={!canCreateInvite}
          >
            Create invite
          </Button>
          <ConfigSelect
            label="Theme"
            value={themeId}
            onChange={(value) => onThemeChange(value as ThemeId)}
            options={discryptUiConfig.themes.map((theme) => ({
              value: theme.id,
              label: theme.label,
            }))}
          />
          <ConfigSelect
            label="Template"
            value={templateId}
            onChange={(value) => onTemplateChange(value as TemplateId)}
            options={discryptUiConfig.templates.map((template) => ({
              value: template.id,
              label: template.label,
            }))}
          />
          <Button
            variant={inspectorOpen ? "secondary" : "outline"}
            size="sm"
            onClick={onToggleInspector}
          >
            Inspector
          </Button>
        </div>
      </div>
    </div>
  );
}

function ConfigSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center gap-2 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.35)] px-2 py-1 text-xs text-[hsl(var(--muted-foreground))]">
      <span className="px-1">{label}</span>
      <select
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
        className="h-8 min-w-36 rounded-lg border-0 bg-transparent px-2 text-[hsl(var(--foreground))] outline-none"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function WorkflowNav({
  workflow,
  setWorkflow,
}: {
  workflow: Workflow;
  setWorkflow: (workflow: Workflow) => void;
}) {
  const items: { id: Workflow; label: string }[] = [
    { id: "setup", label: "Setup" },
    { id: "dm", label: "DMs" },
    { id: "channel", label: "Text" },
    { id: "voice", label: "Voice" },
    { id: "join", label: "Invites" },
    { id: "create-group", label: "Groups" },
  ];
  return (
    <nav
      className="flex gap-2 overflow-x-auto border-b border-[hsl(var(--border))] px-4 py-3 md:px-6"
      aria-label="Workspace sections"
    >
      {items.map((item) => (
        <Button
          key={item.id}
          variant={workflow === item.id ? "secondary" : "ghost"}
          size="sm"
          onClick={() => setWorkflow(item.id)}
        >
          {item.label}
        </Button>
      ))}
    </nav>
  );
}

function SetupPanel({
  snapshot,
  setupSteps,
  completedSteps,
  verifyMessage,
  onVerify,
}: {
  snapshot: AppSnapshot;
  setupSteps: SetupStepView[];
  completedSteps: number;
  verifyMessage: string | null;
  onVerify: () => void;
}) {
  const setupTotal = setupSteps.length;
  const nextStep =
    setupSteps.find((step) => !step.complete) ??
    setupSteps[setupSteps.length - 1];
  const progress = setupTotal > 0 ? (completedSteps / setupTotal) * 100 : 0;
  return (
    <div className="mx-auto grid max-w-6xl gap-5 py-5">
      <Card className="overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.88)] shadow-xl shadow-black/20">
        <CardContent className="grid gap-5 p-5 lg:grid-cols-[1fr_auto] lg:items-center lg:p-6">
          <div className="flex min-w-0 gap-4">
            <div className="grid h-14 w-14 shrink-0 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.35)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]">
              <Icon>□</Icon>
            </div>
            <div className="min-w-0">
              <Badge variant="secondary" className="mb-3 w-fit">
                setup workflow
              </Badge>
              <h2 className="text-2xl font-semibold tracking-tight md:text-3xl">
                Finish the local trust setup
              </h2>
              <p className="mt-2 max-w-3xl text-sm leading-6 text-[hsl(var(--muted-foreground))] md:text-base">
                Verify the current local profile before using chat and voice.
              </p>
            </div>
          </div>
          <div className="min-w-64 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] p-4">
            <div className="flex items-center justify-between gap-4">
              <span className="text-sm font-medium">Progress</span>
              <Badge
                variant={completedSteps === setupTotal ? "success" : "warning"}
              >
                {completedSteps}/{setupTotal}
              </Badge>
            </div>
            <div className="mt-3 h-2 rounded-full bg-[hsl(var(--muted))]">
              <div
                className="h-full rounded-full bg-[hsl(var(--primary))] transition-[width]"
                style={{ width: `${progress}%` }}
              />
            </div>
            <p className="mt-3 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Next: {nextStep?.label ?? "Ready"}
            </p>
          </div>
        </CardContent>
      </Card>
      <div className="grid gap-5 xl:grid-cols-[minmax(0,1.1fr)_minmax(320px,0.9fr)]">
        <Card>
          <CardHeader>
            <CardTitle className="text-2xl">Verify safety numbers</CardTitle>
            <CardDescription>
              Compare this number with {snapshot.friend.alias} in person or over
              a trusted call.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4 lg:grid-cols-[0.95fr_1.05fr]">
            <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.42)] p-4">
              <div className="flex items-center gap-3">
                <Avatar className="h-12 w-12">
                  <AvatarFallback>
                    {snapshot.friend.alias.slice(0, 2).toUpperCase()}
                  </AvatarFallback>
                </Avatar>
                <div>
                  <p className="text-lg font-semibold">
                    {snapshot.friend.alias}
                  </p>
                  <p
                    className={cn(
                      "text-sm",
                      snapshot.friend.verified
                        ? "text-emerald-200"
                        : "text-amber-200",
                    )}
                  >
                    {snapshot.friend.verified ? "Verified" : "Unverified"}
                  </p>
                </div>
              </div>
              <div className="mt-4 rounded-xl border border-[hsl(var(--border))] bg-black/20 p-4">
                <p className="break-words font-mono text-lg font-semibold tracking-[0.12em]">
                  {snapshot.friend.safety_number}
                </p>
                <Button className="mt-4 w-full" onClick={onVerify}>
                  {snapshot.friend.verified ? <Icon>✓</Icon> : <Icon>□</Icon>}{" "}
                  Mark as verified
                </Button>
              </div>
              {verifyMessage ? (
                <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                  {verifyMessage}
                </p>
              ) : null}
            </div>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-1 2xl:grid-cols-2">
              <InfoRow
                title="Device review"
                copy={`${snapshot.devices.length} authorized local device${snapshot.devices.length === 1 ? "" : "s"} available.`}
              />
              <InfoRow
                title="Invite admission"
                copy={snapshot.invite.welcome_required}
              />
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Setup checklist</CardTitle>
            <CardDescription>
              {completedSteps}/{setupTotal} checks complete for this local
              profile.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3">
            {setupSteps.map((step, index) => (
              <div
                key={step.label}
                className={cn(
                  "grid gap-1 rounded-2xl border p-4",
                  step.complete
                    ? "border-emerald-300/25 bg-emerald-300/7"
                    : "border-[hsl(var(--primary)/0.45)] bg-[hsl(var(--primary)/0.08)]",
                )}
              >
                <div className="flex items-center gap-3">
                  <div
                    className={cn(
                      "grid h-9 w-9 shrink-0 place-items-center rounded-xl border text-sm font-semibold",
                      step.complete
                        ? "border-emerald-300/40 bg-emerald-300/10 text-emerald-200"
                        : "border-[hsl(var(--primary)/0.6)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]",
                    )}
                  >
                    {step.complete ? <Icon>✓</Icon> : index + 1}
                  </div>
                  <div className="min-w-0">
                    <p className="font-medium">{step.label}</p>
                    <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))]">
                      {step.detail}
                    </p>
                  </div>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function DmPanel({
  activeDm,
  messages,
  draftDmName,
  setDraftDmName,
  draftMessage,
  setDraftMessage,
  onStartDm,
  onSendDm,
}: {
  activeDm: DirectConversationView | null;
  messages: AppMessageView[];
  draftDmName: string;
  setDraftDmName: (value: string) => void;
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onStartDm: () => void;
  onSendDm: () => void;
}) {
  const visibleMessages = activeDm
    ? messages.filter((message) => message.target.dm_id === activeDm.dm_id)
    : [];
  return (
    <div className="grid min-h-[70dvh] gap-4 py-5 xl:grid-cols-[320px_minmax(0,1fr)]">
      <Card>
        <CardHeader>
          <CardTitle>Direct messages</CardTitle>
          <CardDescription>Local command-backed DM state.</CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">
            Contact name
            <Input
              value={draftDmName}
              onChange={(event) => setDraftDmName(event.target.value)}
            />
          </Label>
          <Button className="mt-4 w-full" onClick={onStartDm}>
            <Icon>+</Icon>Start/open DM
          </Button>
        </CardContent>
      </Card>
      <Timeline
        title={activeDm ? activeDm.display_name : "No DM yet"}
        description={
          activeDm?.local_only_copy ??
          "Start a DM to create a local conversation."
        }
        messages={visibleMessages}
        draftMessage={draftMessage}
        setDraftMessage={setDraftMessage}
        sendLabel="Send DM message"
        onSend={onSendDm}
        disabled={!activeDm}
      />
    </div>
  );
}

function JoinPanel({
  snapshot,
  inviteValue,
  setInviteValue,
  groupName,
  setGroupName,
  latestInvite,
  onJoin,
  onCreateInvite,
  canCreateInvite,
}: {
  snapshot: AppSnapshot;
  inviteValue: string;
  setInviteValue: (value: string) => void;
  groupName: string;
  setGroupName: (value: string) => void;
  latestInvite: string | null;
  onJoin: () => void;
  onCreateInvite: () => void;
  canCreateInvite: boolean;
}) {
  return (
    <div className="grid gap-4 py-5 xl:grid-cols-[minmax(0,1fr)_360px]">
      <Card>
        <CardHeader>
          <CardTitle>Invites and joining</CardTitle>
          <CardDescription>
            Create an invite for the active group or paste an invite to
            join/open a group.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <Label className="grid gap-2">
            Invite URL or code
            <Input
              value={inviteValue}
              onChange={(event) => setInviteValue(event.target.value)}
            />
          </Label>
          <Label className="grid gap-2">
            Joined group label
            <Input
              value={groupName}
              onChange={(event) => setGroupName(event.target.value)}
            />
          </Label>
          <div className="flex flex-wrap gap-2">
            <Button onClick={onJoin}>Join/open group</Button>
            <Button
              variant="outline"
              onClick={onCreateInvite}
              disabled={!canCreateInvite}
            >
              Create invite for active group
            </Button>
            {latestInvite ? (
              <Button
                variant="secondary"
                onClick={() => setInviteValue(latestInvite)}
              >
                Use latest invite
              </Button>
            ) : null}
          </div>
          {latestInvite ? (
            <div className="rounded-2xl border border-emerald-300/30 bg-emerald-300/10 p-4 text-sm text-emerald-100">
              <p className="font-medium">Latest invite</p>
              <p className="mt-1 break-all font-mono text-xs">{latestInvite}</p>
            </div>
          ) : null}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Admission rules</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3">
          <InfoRow title="Expiry" copy={snapshot.invite.expires} />
          <InfoRow title="Max use" copy={snapshot.invite.max_use} />
          <InfoRow
            title="MLS admission"
            copy={snapshot.invite.welcome_required}
          />
        </CardContent>
      </Card>
    </div>
  );
}

function CreateGroupPanel({
  snapshot,
  groupName,
  setGroupName,
  onCreate,
}: {
  snapshot: AppSnapshot;
  groupName: string;
  setGroupName: (value: string) => void;
  onCreate: () => void;
}) {
  return (
    <div className="grid gap-4 py-5 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
      <Card>
        <CardHeader>
          <CardTitle>Create a group</CardTitle>
          <CardDescription>
            Creates a persisted group with default text and voice rooms so the
            workspace is immediately usable.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">
            Group name
            <Input
              value={groupName}
              onChange={(event) => setGroupName(event.target.value)}
            />
          </Label>
          <Button className="mt-5 w-full" onClick={onCreate}>
            Create group
          </Button>
        </CardContent>
      </Card>
      <div className="grid gap-3">
        <InfoRow
          title="Default text channel"
          copy="#general is created for messages."
        />
        <InfoRow
          title="Default voice room"
          copy="Voice Lobby is created for local voice state; no remote participants are invented."
        />
        <InfoRow
          title="Retention warning"
          copy={snapshot.retention.unlimited_warning}
        />
      </div>
    </div>
  );
}

function ChannelPanel({
  group,
  activeChannel,
  channels,
  messages,
  draftChannel,
  setDraftChannel,
  draftMessage,
  setDraftMessage,
  onCreateTextChannel,
  onCreateVoiceChannel,
  onSendMessage,
}: {
  group: GroupView | null;
  activeChannel: ChannelStateView | null;
  channels: ChannelStateView[];
  messages: AppMessageView[];
  draftChannel: string;
  setDraftChannel: (value: string) => void;
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onCreateTextChannel: () => void;
  onCreateVoiceChannel: () => void;
  onSendMessage: () => void;
}) {
  const visibleMessages = activeChannel
    ? messages.filter(
        (message) => message.target.channel_id === activeChannel.channel_id,
      )
    : [];
  return (
    <div className="grid min-h-[72dvh] gap-4 py-5 xl:grid-cols-[minmax(0,1fr)_320px]">
      <Timeline
        title={activeChannel?.name ?? "No text channel"}
        description={
          group
            ? `Group: ${group.name}`
            : "Create or join a group before sending messages."
        }
        messages={visibleMessages}
        draftMessage={draftMessage}
        setDraftMessage={setDraftMessage}
        sendLabel="Send message"
        onSend={onSendMessage}
        disabled={!activeChannel}
      />
      <Card className="h-fit">
        <CardHeader>
          <CardTitle>Channel controls</CardTitle>
          <CardDescription>
            Channels are persisted through the Rust/Tauri command service.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          <Label className="grid gap-2">
            Channel name
            <Input
              value={draftChannel}
              onChange={(event) => setDraftChannel(event.target.value)}
            />
          </Label>
          <div className="grid grid-cols-2 gap-2">
            <Button onClick={onCreateTextChannel} disabled={!group}>
              <Icon>+</Icon>Text
            </Button>
            <Button
              variant="outline"
              onClick={onCreateVoiceChannel}
              disabled={!group}
            >
              <Icon>+</Icon>Voice
            </Button>
          </div>
          <Separator />
          {channels.length === 0 ? (
            <p className="text-sm text-[hsl(var(--muted-foreground))]">
              No text channels yet.
            </p>
          ) : (
            channels.map((channel) => (
              <InfoRow
                key={channel.channel_id}
                title={channel.name}
                copy={channel.retention_status}
              />
            ))
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function Timeline({
  title,
  description,
  messages,
  draftMessage,
  setDraftMessage,
  sendLabel,
  onSend,
  disabled,
}: {
  title: string;
  description: string;
  messages: AppMessageView[];
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  sendLabel: string;
  onSend: () => void;
  disabled?: boolean;
}) {
  return (
    <Card className="flex min-h-[72dvh] flex-col overflow-hidden">
      <CardHeader className="border-b border-[hsl(var(--border))]">
        <CardTitle className="text-xl">{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <ScrollArea className="min-h-0 flex-1 p-4">
        <div className="grid gap-3">
          {messages.length === 0 ? (
            <EmptyState
              title="No messages yet"
              copy="Send the first local command-backed message. It will persist through reloads."
            />
          ) : (
            messages.map((message) => (
              <MessageBubble key={message.message_id} message={message} />
            ))
          )}
        </div>
      </ScrollArea>
      <div className="border-t border-[hsl(var(--border))] p-4">
        <Label className="grid gap-2">
          <span className="sr-only">Message</span>
          <Input
            aria-label="Message"
            value={draftMessage}
            onChange={(event) => setDraftMessage(event.target.value)}
            placeholder="Write a message"
            disabled={disabled}
          />
        </Label>
        <div className="mt-3 flex items-center justify-between gap-3">
          <p className="text-xs text-[hsl(var(--muted-foreground))]">
            Local encrypted timeline facade; remote socket delivery is not
            claimed.
          </p>
          <Button onClick={onSend} disabled={disabled || !draftMessage.trim()}>
            {sendLabel}
          </Button>
        </div>
      </div>
    </Card>
  );
}

function MessageBubble({ message }: { message: AppMessageView }) {
  return (
    <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.34)] p-3">
      <div className="flex items-center justify-between gap-3 text-xs text-[hsl(var(--muted-foreground))]">
        <span>{message.author}</span>
        <span>{message.sent_at}</span>
      </div>
      <p className="mt-1 text-sm leading-6">{message.body}</p>
      <p className="mt-2 text-[11px] text-[hsl(var(--muted-foreground))]">
        {message.status}
      </p>
    </div>
  );
}

function VoicePanel({
  group,
  activeVoiceChannel,
  route,
  participants,
  voiceJoined,
  selfMuted,
  setVoiceJoined,
  setSelfMuted,
  setVolume,
}: {
  group: GroupView | null;
  activeVoiceChannel: ChannelStateView | null;
  route: string;
  participants: VoiceParticipant[];
  voiceJoined: boolean;
  selfMuted: boolean;
  setVoiceJoined: (joined: boolean) => void;
  setSelfMuted: (muted: boolean) => void;
  setVolume: (id: string, value: number[]) => void;
}) {
  const visibleParticipants = voiceJoined ? participants : [];
  return (
    <div className="grid gap-4 py-5 xl:grid-cols-[minmax(0,1fr)_340px]">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div>
              <CardTitle>{activeVoiceChannel?.name ?? "Voice Lobby"}</CardTitle>
              <CardDescription>
                {group ? route : "Create or join a group before voice."}
              </CardDescription>
            </div>
            <Badge variant={voiceJoined ? "success" : "secondary"}>
              {voiceJoined ? "joined" : "not joined"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3">
          {!voiceJoined ? (
            <EmptyState
              title="Not in voice"
              copy="Join to create a local voice session. No remote Bob/relay members are fabricated."
            />
          ) : null}
          {voiceJoined && visibleParticipants.length === 0 ? (
            <EmptyState
              title="No local participants"
              copy="The backend returned an empty participant list."
            />
          ) : null}
          {visibleParticipants.map((participant) => (
            <div
              key={participant.id}
              className="grid gap-3 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4 md:grid-cols-[1fr_180px] md:items-center"
            >
              <div className="flex items-center gap-3">
                <div
                  className={cn(
                    "rounded-2xl p-0.5",
                    participant.speaking &&
                      !participant.muted &&
                      "bg-emerald-300/70",
                  )}
                >
                  <Avatar>
                    <AvatarFallback>
                      {participant.name.slice(0, 2).toUpperCase()}
                    </AvatarFallback>
                  </Avatar>
                </div>
                <div>
                  <p className="font-medium">
                    {participant.name}{" "}
                    <span className="text-xs text-[hsl(var(--muted-foreground))]">
                      · {participant.role}
                    </span>
                  </p>
                  <p className="text-xs text-[hsl(var(--muted-foreground))]">
                    {participant.muted
                      ? "muted"
                      : participant.speaking
                        ? "speaking now"
                        : "listening"}
                  </p>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <Icon>vol</Icon>
                <Slider
                  value={[participant.volume]}
                  min={0}
                  max={100}
                  step={1}
                  onValueChange={(value) => setVolume(participant.id, value)}
                />
              </div>
            </div>
          ))}
        </CardContent>
      </Card>
      <Card className="h-fit">
        <CardHeader>
          <CardTitle>Call controls</CardTitle>
          <CardDescription>
            All controls dispatch command-backed state changes.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-5">
          <ControlRow
            label="Mute my microphone"
            checked={selfMuted}
            onCheckedChange={setSelfMuted}
            disabled={!voiceJoined}
          />
          <Button
            variant={voiceJoined ? "destructive" : "default"}
            onClick={() => setVoiceJoined(!voiceJoined)}
            disabled={!group}
          >
            {voiceJoined ? "Leave call" : "Join call"}
          </Button>
          <InfoRow
            title="Voice honesty"
            copy="This build persists local voice session controls only. Real media transport is release-gated behind media-frame E2E and no relay/member is shown unless returned by state."
          />
        </CardContent>
      </Card>
    </div>
  );
}

function InspectorRail({
  snapshot,
  appState,
  participants,
  completedSteps,
  themeLabel,
  templateLabel,
}: {
  snapshot: AppSnapshot;
  appState: AppState;
  participants: VoiceParticipant[];
  completedSteps: number;
  themeLabel: string;
  templateLabel: string;
}) {
  const latestEvents = useMemo(
    () => appState.events.slice(-10).reverse(),
    [appState.events],
  );
  return (
    <aside className="hidden h-dvh border-l border-[hsl(var(--border))] bg-[hsl(var(--card)/0.62)] p-4 backdrop-blur-xl 2xl:block">
      <ScrollArea className="h-full">
        <div className="grid gap-4">
          <Card>
            <CardHeader>
              <CardTitle>Workspace state</CardTitle>
              <CardDescription>
                {completedSteps}/4 setup checks · {themeLabel} · {templateLabel}
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3">
              <InfoRow
                title="Groups"
                copy={`${appState.groups.length} persisted group${appState.groups.length === 1 ? "" : "s"}`}
              />
              <InfoRow
                title="Messages"
                copy={`${appState.messages.length} local message${appState.messages.length === 1 ? "" : "s"}`}
              />
              <InfoRow
                title="Voice members"
                copy={`${participants.length} state-backed participant${participants.length === 1 ? "" : "s"}`}
              />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Security copy</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              <p>{snapshot.security_copy.metadata}</p>
              <Separator />
              <p>{snapshot.security_copy.deletion}</p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Activity</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-2">
              {latestEvents.map((event) => (
                <p
                  key={event.sequence}
                  className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.4)] p-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]"
                >
                  {event.summary}
                </p>
              ))}
            </CardContent>
          </Card>
        </div>
      </ScrollArea>
    </aside>
  );
}

function InfoRow({ title, copy }: { title: string; copy: string }) {
  return (
    <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
      <p className="font-medium">{title}</p>
      <p className="mt-1 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
        {copy}
      </p>
    </div>
  );
}
function EmptyState({ title, copy }: { title: string; copy: string }) {
  return (
    <div className="grid place-items-center rounded-2xl border border-dashed border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.22)] p-8 text-center">
      <div>
        <p className="font-semibold">{title}</p>
        <p className="mt-2 max-w-md text-sm leading-6 text-[hsl(var(--muted-foreground))]">
          {copy}
        </p>
      </div>
    </div>
  );
}
function ControlRow({
  label,
  checked,
  onCheckedChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
      <span className="text-sm font-medium">{label}</span>
      <Switch
        aria-label={label}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
