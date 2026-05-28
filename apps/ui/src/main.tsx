import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { AppSnapshot, loadAppSnapshot, verifySafetyNumber } from './commands';
import './styles.css';

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    loadAppSnapshot()
      .then((loaded) => {
        if (mounted) {
          setSnapshot(loaded);
        }
      })
      .catch((error: unknown) => {
        if (mounted) {
          setLoadError(error instanceof Error ? error.message : 'Unable to load app snapshot');
        }
      });
    return () => {
      mounted = false;
    };
  }, []);

  if (loadError) {
    return <main className="loading error">discrypt command surface failed: {loadError}</main>;
  }

  if (!snapshot) {
    return <main className="loading">Loading discrypt…</main>;
  }

  const currentSnapshot = snapshot;
  const activeServer = currentSnapshot.servers[0];
  const textChannels = activeServer.channels.filter((channel) => channel.kind === 'Text');
  const voiceChannels = activeServer.channels.filter((channel) => channel.kind === 'Voice');

  async function confirmSafetyNumber() {
    try {
      const result = await verifySafetyNumber({
        friend_id: currentSnapshot.friend.friend_code,
        provided: currentSnapshot.friend.safety_number,
      });
      setVerifyMessage(result.message);
      if (result.verified) {
        setSnapshot({
          ...currentSnapshot,
          friend: { ...currentSnapshot.friend, verified: true },
        });
      }
    } catch (error: unknown) {
      setVerifyMessage(
        `Safety verification command failed: ${error instanceof Error ? error.message : 'unknown error'}`,
      );
    }
  }

  return (
    <main className="app-shell">
      <aside className="server-rail" aria-label="Servers">
        <div className="brand">d</div>
        {currentSnapshot.servers.map((server) => (
          <button className="server-pill active" key={server.name} type="button" title={server.name}>
            {server.name.slice(0, 2).toUpperCase()}
          </button>
        ))}
      </aside>

      <aside className="sidebar">
        <header>
          <h1>discrypt</h1>
          <p>{activeServer.name} · {activeServer.role}</p>
        </header>
        <section>
          <h2>Text channels</h2>
          {textChannels.map((channel) => (
            <button className="channel" key={channel.name} type="button">
              <span>{channel.name}</span>
              <small>{channel.retention_status}</small>
            </button>
          ))}
        </section>
        <section>
          <h2>Voice</h2>
          {voiceChannels.map((channel) => (
            <button className="channel voice" key={channel.name} type="button">
              <span>{channel.name}</span>
              <small>{snapshot.voice.route}</small>
            </button>
          ))}
        </section>
      </aside>

      <section className="content">
        <header className="hero">
          <div>
            <p className="eyebrow">Safety-number verification</p>
            <h2>{snapshot.friend.alias}</h2>
            <p>Safety number: <strong>{snapshot.friend.safety_number}</strong></p>
          </div>
          <div className="verify-actions">
            <span className={currentSnapshot.friend.verified ? 'status good' : 'status pending'}>{currentSnapshot.friend.verified ? 'verified' : 'unverified'}</span>
            <button className="verify-button" type="button" onClick={confirmSafetyNumber}>I compared this safety number</button>
            {verifyMessage ? <small>{verifyMessage}</small> : null}
          </div>
        </header>

        <section className="grid">
          <Card title="Invite admission">
            <ul>
              <li>{snapshot.invite.expires}</li>
              <li>{snapshot.invite.max_use}</li>
              <li>{snapshot.invite.password_gate}</li>
              <li>{snapshot.invite.welcome_required}</li>
            </ul>
          </Card>

          <Card title="Devices">
            <ul>
              {currentSnapshot.devices.map((device) => (
                <li key={device.device_id}>
                  <strong>{device.device_id}</strong> · leaf {device.leaf_index} · {device.local ? 'local' : 'remote'} · {device.authorized ? 'authorized' : 'blocked'}
                </li>
              ))}
            </ul>
          </Card>

          <Card title="Retention">
            <p>Selected: <strong>{currentSnapshot.retention.selected}</strong></p>
            <p>{currentSnapshot.retention.transition_copy}</p>
            <p className="warning">{currentSnapshot.retention.unlimited_warning}</p>
          </Card>

          <Card title="Voice and relay security">
            <p>{currentSnapshot.voice.relay_copy}</p>
            <p>{currentSnapshot.voice.android_path}</p>
          </Card>

          <Card title="Connectivity and wake">
            <p>{currentSnapshot.connectivity.fallback_chain}</p>
            <p>{currentSnapshot.connectivity.push_copy}</p>
          </Card>

          <Card title="Honest guarantees">
            <p>{currentSnapshot.security_copy.metadata}</p>
            <p>{currentSnapshot.security_copy.deletion}</p>
            <p>{currentSnapshot.security_copy.malicious_member}</p>
          </Card>
        </section>
      </section>
    </main>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <article className="card">
      <h3>{title}</h3>
      {children}
    </article>
  );
}

createRoot(document.getElementById('root')!).render(<App />);
