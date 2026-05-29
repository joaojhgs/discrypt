import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  CheckCircledIcon,
  GearIcon,
  LockClosedIcon,
  PersonIcon,
  PlusIcon,
  SpeakerLoudIcon,
  SpeakerOffIcon,
} from "@radix-ui/react-icons";
import { discryptUiConfig, ThemeId, TemplateId } from "./app-config";
import {
  activityFeed,
  discryptUiConfig,
  setupChecklist,
  ThemeId,
  TemplateId,
} from "./app-config";
import {
  AppSnapshot,
  AppState,
  ChannelView,
  MessageView,
  VoiceParticipantView,
  VoiceSessionView,
  createChannel,
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
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import "./styles.css";

type Workflow = "setup" | "dm" | "join" | "create-group" | "channel" | "voice";

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

function App() {
  const [commandState, setCommandState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [view, setView] = useState<View>("setup");
  const [selectedDmId, setSelectedDmId] = useState<string | null>(null);
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [selectedChannelId, setSelectedChannelId] = useState<string | null>(null);
  const [draftMessage, setDraftMessage] = useState("Hello from discrypt");
  const [draftDmPeer, setDraftDmPeer] = useState("Bob");
  const [draftGroup, setDraftGroup] = useState("private lab");
  const [draftInvite, setDraftInvite] = useState("discrypt://join/local-template");
  const [draftChannel, setDraftChannel] = useState("ops-room");
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [workflow, setWorkflow] = useState<Workflow>('setup');
  const [draftChannel, setDraftChannel] = useState('secure-room');
  const [draftMessage, setDraftMessage] = useState('Hello from the command-backed UI');
  const [draftGroup, setDraftGroup] = useState('private lab');
  const [draftInvite, setDraftInvite] = useState('invite:joined-enclave');
  const [draftDisplayName, setDraftDisplayName] = useState('Alice');
  const [draftDeviceName, setDraftDeviceName] = useState('Desktop');
  const [draftRecoveryCode, setDraftRecoveryCode] = useState('local recovery placeholder');
  const [draftDmName, setDraftDmName] = useState('Bob');

  useEffect(() => {
    let mounted = true;
    loadAppState()
      .then((loaded: AppState) => {
        if (mounted) {
          setCommandState(loaded);
        }
      })
      .catch((error: unknown) => setLoadError(error instanceof Error ? error.message : "Unable to load app state"));
    return () => {
      mounted = false;
    };
  }, []);

  async function applyCommand(command: Promise<AppState>, success?: (state: AppState) => void) {
    try {
      setCommandError(null);
      const next = await command;
      setState(next);
      after?.(next);
    } catch (error: unknown) {
      setCommandError(error instanceof Error ? error.message : "Command failed");
    }
  }

  if (loadError) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-red-200">discrypt failed to load: {loadError}</main>;
  }
  if (!state) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">Loading discrypt…</main>;
  }

  const appState = commandState;
  const currentSnapshot = appState.snapshot;
  const activeGroup = appState.active_context?.group_id
    ? appState.groups.find((group) => group.group_id === appState.active_context?.group_id) ?? appState.groups[0] ?? null
    : appState.groups[0] ?? null;
  const activeServer = currentSnapshot.servers[0] ?? { name: 'No group selected', role: 'local profile', channels: [] };
  const textChannels = activeServer.channels.filter((channel) => channel.kind === 'Text');
  const voiceChannels = activeServer.channels.filter((channel) => channel.kind === 'Voice');
  const activeTextChannel = activeGroup?.channels.find((channel) => channel.kind === 'Text') ?? null;
  const activeVoiceChannel = activeGroup?.channels.find((channel) => channel.kind === 'Voice') ?? null;
  const groupLabel = activeGroup?.name ?? 'Local profile';
  const participants = appState.voice_session?.participants ?? currentSnapshot.voice_session.participants;
  const voiceJoined = appState.voice_session?.joined ?? false;
  const selfMuted = appState.voice_session?.self_muted ?? participants.find((participant) => participant.id === 'local-user' || participant.id === 'alice')?.muted ?? false;
  const activeTheme = discryptUiConfig.themes.find((theme) => theme.id === appState.preferences.theme_id) ?? discryptUiConfig.themes[0];
  const activeTemplate = discryptUiConfig.templates.find((template) => template.id === appState.preferences.template_id) ?? discryptUiConfig.templates[0];
  const themeStyle = activeTheme.cssVars as React.CSSProperties;
  const completedSteps = [
    appState.profile !== null,
    currentSnapshot.friend.verified,
    appState.devices.length >= 1,
    currentSnapshot.invite.welcome_required.length > 0,
    currentSnapshot.retention.selected.length > 0,
  ].filter(Boolean).length;

  async function confirmSafetyNumber() {
    try {
      const result = await verifySafetyNumber({
        friend_id: currentSnapshot.friend.friend_code,
        provided: currentSnapshot.friend.safety_number,
      });
      setVerifyMessage(result.message);
      if (result.verified) {
        await applyCommand(loadAppState());
      }
    } catch (error: unknown) {
      setVerifyMessage(`Safety verification command failed: ${error instanceof Error ? error.message : 'unknown error'}`);
    }
    return null;
  }

  function createCommandUser() {
    void applyCommand(createUser({ display_name: draftDisplayName, device_name: draftDeviceName }), () => setWorkflow('setup'));
  }

  function recoverCommandUser() {
    void applyCommand(recoverUser({ display_name: draftDisplayName, device_name: draftDeviceName, recovery_code: draftRecoveryCode }), () => setWorkflow('setup'));
  }

  function createCommandGroup() {
    void applyCommand(createGroup({ name: draftGroup, retention: currentSnapshot.retention.selected }), () => setWorkflow('channel'));
  }

  function joinCommandGroup() {
    void applyCommand(joinGroup({ invite_code: draftInvite, group_name: draftInvite.includes('enclave') ? 'joined enclave' : 'joined group' }), () => setWorkflow('setup'));
  }

  function startCommandDm() {
    void applyCommand(startDm({ display_name: draftDmName }), () => setWorkflow('dm'));
  }

  function createCommandChannel() {
    if (!activeGroup) {
      setCommandError('Create or join a group before adding a channel.');
      return;
    }
    const name = draftChannel.trim().replace(/^#/, '') || 'secure-room';
    void applyCommand(createChannelCommand({ group_id: activeGroup.group_id, name, kind: 'Text', retention_status: currentSnapshot.retention.selected }), () => setWorkflow('channel'));
  }

  function sendCommandMessage(channelName: string) {
    const body = draftMessage.trim();
    if (!body) return;
    if (!activeGroup || !activeTextChannel) {
      setCommandError('Create a text channel before sending a group message.');
      return;
    }
    void applyCommand(sendMessage({
      target: { kind: 'channel', dm_id: null, group_id: activeGroup.group_id, channel_id: activeTextChannel.channel_id },
      body,
    }), () => setDraftMessage(''));
  }

  function sendCommandDm() {
    const body = draftMessage.trim();
    const dm = appState.dms[0];
    if (!body || !dm) return;
    void applyCommand(sendMessage({
      target: { kind: 'dm', dm_id: dm.dm_id, group_id: null, channel_id: null },
      body,
    }), () => setDraftMessage(''));
  }

  function createCommandInvite() {
    if (!activeGroup) {
      setCommandError('Create or join a group before creating an invite.');
      return;
    }
    void applyCommand(createInvite({ group_id: activeGroup.group_id, expires: currentSnapshot.invite.expires, max_use: currentSnapshot.invite.max_use }), () => setWorkflow('join'));
  }

  function setVolume(id: string, value: number[]) {
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) {
      setCommandError('Join a voice channel before changing volume.');
      return;
    }
    void applyCommand(setSpeakerVolume({ session_id: sessionId, participant_id: id, volume: value[0] ?? 0 }));
  }

  function toggleSelfMute(checked: boolean) {
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) {
      setCommandError('Join a voice channel before muting.');
      return;
    }
    void applyCommand(setSelfMute({ session_id: sessionId, muted: checked }));
  }

  async function toggleVoiceJoin(joined: boolean) {
    if (joined) {
      if (!activeGroup) {
        setCommandError('Create or join a group before joining voice.');
        return;
      }
      let voiceChannel = activeVoiceChannel;
      if (!voiceChannel) {
        const withVoice = await createChannelCommand({ group_id: activeGroup.group_id, name: 'Voice Lobby', kind: 'Voice', retention_status: 'session' });
        setCommandState(withVoice);
        voiceChannel = withVoice.groups.find((group) => group.group_id === activeGroup.group_id)?.channels.find((channel) => channel.kind === 'Voice') ?? null;
      }
      if (!voiceChannel) {
        setCommandError('Voice channel creation did not return a channel.');
        return;
      }
      void applyCommand(joinVoice({ group_id: activeGroup.group_id, channel_id: voiceChannel.channel_id }), () => setWorkflow('voice'));
      return;
    }
    const sessionId = appState.voice_session?.session_id;
    if (!sessionId) return;
    void applyCommand(leaveVoice({ session_id: sessionId }), () => setWorkflow('voice'));
  }

  function chooseTheme(nextTheme: ThemeId) {
    void applyCommand(savePreferences({ theme_id: nextTheme, template_id: activeTemplate.id }));
  }

  function chooseTemplate(nextTemplate: TemplateId) {
    void applyCommand(savePreferences({ theme_id: activeTheme.id, template_id: nextTemplate }));
  }

  if (appState.lifecycle === 'first_run') {
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
    <TooltipProvider delayDuration={150}>
      <main
        data-template={activeTemplate.id}
        style={themeStyle}
        className={cn(
          "grid min-h-dvh grid-cols-[72px_minmax(260px,330px)_minmax(0,1fr)_minmax(280px,340px)] overflow-hidden bg-[hsl(var(--background))] text-[hsl(var(--foreground))]",
          activeTemplate.density === "compact" && "grid-cols-[64px_minmax(230px,300px)_minmax(0,1fr)_minmax(260px,310px)]",
        )}
      >
        <ServerRail groups={state.groups} activeGroupId={activeGroup?.group_id ?? null} onSelectGroup={(groupId) => { setSelectedGroupId(groupId); setView("group"); }} onDm={() => setView("dm")} />
        <Sidebar
          user={state.user.display_name}
          dms={state.dms}
          groups={state.groups}
          activeDmId={activeDm?.dm_id ?? null}
          activeGroupId={activeGroup?.group_id ?? null}
          activeChannelId={activeTextChannel?.channel_id ?? null}
          activeVoiceSession={activeVoiceSession}
          onSelectDm={(dmId) => { setSelectedDmId(dmId); setView("dm"); }}
          onSelectGroup={(groupId) => { setSelectedGroupId(groupId); setView("group"); }}
          onSelectText={(groupId, channelId) => { setSelectedGroupId(groupId); setSelectedChannelId(channelId); setView("group"); }}
          onSelectVoice={(groupId) => { setSelectedGroupId(groupId); setView("voice"); }}
        />
        <ScrollArea className="h-dvh">
          <section className="min-h-dvh bg-[radial-gradient(circle_at_80%_0%,hsl(var(--primary)/0.10),transparent_34rem)] p-4 pb-52 md:p-6 md:pb-56">
            <TopBar
              user={state.user.display_name}
              activeTheme={activeTheme.id as ThemeId}
              activeTemplate={activeTemplate.id as TemplateId}
              onThemeChange={chooseTheme}
              onTemplateChange={chooseTemplate}
              onSetup={() => setView("setup")}
            />
            {commandError ? <p className="mt-3 rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100">Command note: {commandError}</p> : null}
            {appState.invites[0] ? <p className="mt-3 rounded-xl border border-emerald-300/30 bg-emerald-300/10 p-3 text-sm text-emerald-100">Invite ready: {appState.invites[0].code}</p> : null}
            <Tabs value={workflow} onValueChange={(value) => setWorkflow(value as Workflow)} className="mt-5">
              <TabsList className="flex w-full justify-start overflow-x-auto md:w-auto">
                <TabsTrigger value="setup">Setup</TabsTrigger>
                <TabsTrigger value="dm">DMs</TabsTrigger>
                <TabsTrigger value="join">Join</TabsTrigger>
                <TabsTrigger value="create-group">Create group</TabsTrigger>
                <TabsTrigger value="channel">Channels</TabsTrigger>
                <TabsTrigger value="voice">Voice</TabsTrigger>
              </TabsList>
              <TabsContent value="setup">
                <ReadySetupPanel
                  state={state}
                  verifyMessage={verifyMessage}
                  onVerify={async () => {
                    const result = await verifySafetyNumber({ friend_id: state.snapshot.friend.friend_code, provided: state.snapshot.friend.safety_number });
                    setVerifyMessage(result.message);
                    setState(await loadAppState());
                  }}
                />
              </TabsContent>
              <TabsContent value="dm">
                <DmPanel dms={appState.dms} messages={appState.messages} draftDmName={draftDmName} setDraftDmName={setDraftDmName} draftMessage={draftMessage} setDraftMessage={setDraftMessage} onStartDm={startCommandDm} onSendDm={sendCommandDm} />
              </TabsContent>
              <TabsContent value="join">
                <JoinPanel snapshot={currentSnapshot} onJoin={joinCommandGroup} onCreateInvite={createCommandInvite} />
              </TabsContent>
              <TabsContent value="group">
                <GroupPanel
                  group={activeGroup}
                  textChannel={activeTextChannel}
                  messages={filterMessages(state.messages, activeGroup && activeTextChannel ? { kind: "channel", group_id: activeGroup.group_id, channel_id: activeTextChannel.channel_id } : null)}
                  activeInvite={state.active_invite}
                  retention={state.snapshot.retention.selected}
                  draftGroup={draftGroup}
                  setDraftGroup={setDraftGroup}
                  draftInvite={draftInvite}
                  setDraftInvite={setDraftInvite}
                  draftChannel={draftChannel}
                  setDraftChannel={setDraftChannel}
                  draftMessage={draftMessage}
                  setDraftMessage={setDraftMessage}
                  onCreateGroup={() => apply(createGroup({ name: draftGroup, retention: state.snapshot.retention.selected }), (next) => setSelectedGroupId(next.active_group_id ?? next.groups.at(-1)?.group_id ?? null))}
                  onJoinGroup={() => apply(joinGroup({ invite_code: draftInvite }), (next) => setSelectedGroupId(next.active_group_id ?? next.groups.at(-1)?.group_id ?? null))}
                  onCreateInvite={() => activeGroup && apply(createInvite({ group_id: activeGroup.group_id, expires: state.snapshot.invite.expires, max_use: state.snapshot.invite.max_use }))}
                  onCreateText={() => activeGroup && apply(createChannel({ group_id: activeGroup.group_id, name: draftChannel, kind: "Text", retention_status: state.snapshot.retention.selected }), (next) => {
                    const group = next.groups.find((item) => item.group_id === activeGroup.group_id);
                    setSelectedChannelId(group?.channels.filter((channel) => channel.kind === "Text").at(-1)?.channel_id ?? null);
                  })}
                  onCreateVoice={() => activeGroup && apply(createChannel({ group_id: activeGroup.group_id, name: draftChannel || "Voice Room", kind: "Voice", retention_status: "Session-state only" }))}
                  onSend={submitMessage}
                />
              </TabsContent>
              <TabsContent value="voice">
                <VoicePanel
                  group={activeGroup}
                  voiceChannels={voiceChannels}
                  sessions={state.voice_sessions}
                  activeSession={activeVoiceSession}
                  onJoin={(channel) => activeGroup && apply(joinVoice({ group_id: activeGroup.group_id, channel_id: channel.channel_id }))}
                  onLeave={(session) => apply(leaveVoice({ session_id: session.session_id }))}
                  onMute={(session, muted) => apply(setSelfMute({ session_id: session.session_id, muted }))}
                  onVolume={(session, participant, volume) => apply(setSpeakerVolume({ session_id: session.session_id, participant_id: participant.id, volume }))}
                />
              </TabsContent>
            </Tabs>
          </section>
        </ScrollArea>
        <RightRail state={state} activeTheme={activeTheme.label} activeTemplate={activeTemplate.label} />
        {activeVoiceSession ? <VoiceDock session={activeVoiceSession} onLeave={() => void apply(leaveVoice({ session_id: activeVoiceSession.session_id }))} onMute={(muted) => void apply(setSelfMute({ session_id: activeVoiceSession.session_id, muted }))} /> : null}
      </main>
    </TooltipProvider>
  );
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
      className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]"
    >
      <Card className="w-full max-w-3xl border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.88)] shadow-2xl">
        <CardHeader>
          <Badge variant="secondary" className="w-fit">first run</Badge>
          <CardTitle className="text-3xl">Set up your local discrypt profile</CardTitle>
          <CardDescription>
            Create a new local profile or recover with a test-build placeholder. Recovery does not claim cloud backup, history restore, or cross-device key recovery.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4 md:grid-cols-2">
          {commandError ? <p className="md:col-span-2 rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100">Command note: {commandError}</p> : null}
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
            <Label className="grid gap-2">Display name<Input value={displayName} onChange={(event) => setDisplayName(event.target.value)} /></Label>
            <Label className="mt-4 grid gap-2">Device name<Input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} /></Label>
            <Button className="mt-5 w-full" onClick={onCreate}>Create new user</Button>
          </div>
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
            <Label className="grid gap-2">Recovery phrase/code<Input value={recoveryCode} onChange={(event) => setRecoveryCode(event.target.value)} /></Label>
            <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              Local/test-build placeholder only. It unlocks the shell for E2E coverage but does not recover remote devices or message history.
            </p>
            <Button variant="outline" className="mt-5 w-full" onClick={onRecover}>Recover existing user</Button>
          </div>
        </CardContent>
      </Card>
    </main>
  );
}

function ServerRail({
  groupLabel,
  themeLabel,
}: {
  groupLabel: string;
  themeLabel: string;
}) {
  return (
    <div className="mx-auto grid min-h-[calc(100dvh-3rem)] max-w-5xl place-items-center">
      <Card className="w-full overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.86)]">
        <CardHeader>
          <div className="flex items-start gap-4">
            <div className="grid h-14 w-14 place-items-center rounded-2xl bg-[hsl(var(--primary))] text-2xl font-black text-[hsl(var(--primary-foreground))]">d</div>
            <div>
              <CardTitle className="text-3xl">Welcome to discrypt</CardTitle>
              <CardDescription>First choose a local user. QR/mobile pairing is intentionally disabled in this build.</CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {commandError ? <p className="mb-4 rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100">{commandError}</p> : null}
          <Tabs defaultValue="new" className="grid gap-4">
            <TabsList><TabsTrigger value="new">Setup new user</TabsTrigger><TabsTrigger value="recover">Use existing user</TabsTrigger></TabsList>
            <div className="grid gap-4 md:grid-cols-2">
              <Card className="shadow-none"><CardHeader><CardTitle>Local identity</CardTitle><CardDescription>{recoveryCopy}</CardDescription></CardHeader></Card>
              <TabsContent value="new" className="mt-0 grid gap-3">
                <Field label="Display name" value={display} onChange={setDisplay} />
                <Field label="Device name" value={device} onChange={setDevice} />
                <Button size="lg" onClick={() => onCreate(display, device)}><PlusIcon /> Create user</Button>
              </TabsContent>
              <TabsContent value="recover" className="mt-0 grid gap-3">
                <Field label="Display name" value={display} onChange={setDisplay} />
                <Field label="Device name" value={device} onChange={setDevice} />
                <Field label="Recovery code" value={code} onChange={setCode} />
                <Button size="lg" variant="secondary" onClick={() => onRecover(display, device, code)}><PersonIcon /> Recover local user</Button>
              </TabsContent>
            </div>
          </Tabs>
        </CardContent>
      </Card>
    </div>
  );
}

function ServerRail({ groups, activeGroupId, onSelectGroup, onDm }: { groups: GroupView[]; activeGroupId: string | null; onSelectGroup: (id: string) => void; onDm: () => void }) {
  return <aside className="hidden border-r border-[hsl(var(--border))] bg-black/20 p-3 md:flex md:flex-col md:items-center md:gap-3">
    <Button size="icon" className="h-11 w-11 rounded-2xl font-black" onClick={onDm}>d</Button>
    {groups.map((group) => <Tooltip key={group.group_id}><TooltipTrigger asChild><Button variant={group.group_id === activeGroupId ? "secondary" : "outline"} size="icon" className="h-11 w-11 rounded-2xl text-xs font-bold" onClick={() => onSelectGroup(group.group_id)}>{group.name.slice(0, 2).toUpperCase()}</Button></TooltipTrigger><TooltipContent side="right">{group.name}</TooltipContent></Tooltip>)}
    <div className="mt-auto grid h-10 w-10 place-items-center rounded-xl border border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]"><GearIcon /></div>
  </aside>;
}

function Sidebar(props: { user: string; dms: DmView[]; groups: GroupView[]; activeDmId: string | null; activeGroupId: string | null; activeChannelId: string | null; activeVoiceSession: VoiceSessionView | null; onSelectDm: (id: string) => void; onSelectGroup: (id: string) => void; onSelectText: (group: string, channel: string) => void; onSelectVoice: (group: string) => void }) {
  return <aside className="hidden h-dvh border-r border-[hsl(var(--border))] bg-[hsl(var(--card)/0.58)] backdrop-blur-xl lg:block"><div className="flex h-full flex-col"><div className="border-b border-[hsl(var(--border))] p-4"><h1 className="text-lg font-semibold">{props.user}</h1><p className="text-xs text-[hsl(var(--muted-foreground))]">local user · encrypted-first shell</p></div><ScrollArea className="flex-1 p-3">
    <SectionLabel>Direct messages</SectionLabel>{props.dms.map((dm) => <SidebarButton key={dm.dm_id} active={dm.dm_id === props.activeDmId} onClick={() => props.onSelectDm(dm.dm_id)}>@ {dm.peer_label}</SidebarButton>)}
    <SectionLabel>Groups</SectionLabel>{props.groups.map((group) => <div key={group.group_id}><SidebarButton active={group.group_id === props.activeGroupId} onClick={() => props.onSelectGroup(group.group_id)} meta={group.role}>{group.name}</SidebarButton>{group.channels.filter((channel) => channel.kind === "Text").map((channel) => <SidebarButton key={channel.channel_id} active={channel.channel_id === props.activeChannelId} onClick={() => props.onSelectText(group.group_id, channel.channel_id)} meta={channel.retention_status}>{channel.name}</SidebarButton>)}{group.channels.filter((channel) => channel.kind === "Voice").map((channel) => <SidebarButton key={channel.channel_id} active={props.activeVoiceSession?.channel_id === channel.channel_id} onClick={() => props.onSelectVoice(group.group_id)} meta={props.activeVoiceSession?.channel_id === channel.channel_id ? "joined" : "ready"}>{channel.name}</SidebarButton>)}</div>)}
  </ScrollArea></div></aside>;
}

function TopBar({ user, activeTheme, activeTemplate, onThemeChange, onTemplateChange, onSetup }: { user: string; activeTheme: ThemeId; activeTemplate: TemplateId; onThemeChange: (id: ThemeId) => void; onTemplateChange: (id: TemplateId) => void; onSetup: () => void }) {
  return <Card className="sticky top-4 z-20 border-[hsl(var(--border)/0.8)] bg-[hsl(var(--card)/0.9)]"><div className="flex flex-col gap-3 p-3 xl:flex-row xl:items-center xl:justify-between"><div className="flex items-center gap-3"><div className="grid h-10 w-10 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.4)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]"><LockClosedIcon /></div><div><h2 className="text-xl font-semibold">{user}'s discrypt</h2><p className="text-xs text-[hsl(var(--muted-foreground))]">State-managed DMs, groups, text, and voice</p></div></div><div className="flex flex-wrap items-center gap-2"><Button variant="outline" size="sm" onClick={onSetup}>Setup</Button><ConfigSelect label="Theme" value={activeTheme} onChange={(value) => onThemeChange(value as ThemeId)} options={discryptUiConfig.themes.map((theme) => ({ value: theme.id, label: theme.label }))} /><ConfigSelect label="Template" value={activeTemplate} onChange={(value) => onTemplateChange(value as TemplateId)} options={discryptUiConfig.templates.map((template) => ({ value: template.id, label: template.label }))} /></div></div></Card>;
}

function ReadySetupPanel({ state, verifyMessage, onVerify }: { state: AppStateView; verifyMessage: string | null; onVerify: () => void }) {
  return <div className="grid gap-4 xl:grid-cols-2"><Card><CardHeader><CardTitle>Identity and recovery</CardTitle><CardDescription>{state.recovery_copy}</CardDescription></CardHeader><CardContent className="grid gap-3"><Badge variant="success">{state.user?.display_name} · {state.user?.device_name}</Badge><p className="text-sm text-[hsl(var(--muted-foreground))]">{state.user?.recovery_hint}</p></CardContent></Card><Card><CardHeader><CardTitle>Safety number</CardTitle><CardDescription>Verify before trusting DMs or group membership.</CardDescription></CardHeader><CardContent className="grid gap-3"><code className="text-lg">{state.snapshot.friend.safety_number}</code><Button onClick={onVerify}><CheckCircledIcon /> Verify fixture safety number</Button>{verifyMessage ? <p className="text-sm text-emerald-200">{verifyMessage}</p> : null}</CardContent></Card></div>;
}

function DmPanel(props: { dms: DmView[]; activeDm: DmView | null; messages: MessageView[]; draftPeer: string; setDraftPeer: (v: string) => void; draftMessage: string; setDraftMessage: (v: string) => void; onStartDm: () => void; onSend: () => void; onSelect: (id: string) => void }) {
  return <div className="grid gap-4 xl:grid-cols-[0.7fr_1.3fr]"><Card><CardHeader><CardTitle>Direct messages</CardTitle><CardDescription>Start a local command-backed DM.</CardDescription></CardHeader><CardContent className="grid gap-3"><Field label="Peer" value={props.draftPeer} onChange={props.setDraftPeer} /><Button onClick={props.onStartDm}><PlusIcon /> Start DM</Button><Separator />{props.dms.map((dm) => <Button key={dm.dm_id} variant={dm.dm_id === props.activeDm?.dm_id ? "secondary" : "ghost"} onClick={() => props.onSelect(dm.dm_id)}>@ {dm.peer_label}</Button>)}</CardContent></Card><ChatCard title={props.activeDm ? `@ ${props.activeDm.peer_label}` : "No DM selected"} messages={props.messages} draft={props.draftMessage} setDraft={props.setDraftMessage} onSend={props.onSend} /></div>;
}

function GroupPanel(props: { group: GroupView | null; textChannel: AppChannelView | null; messages: MessageView[]; activeInvite: { code: string; admission_copy: string } | null; retention: string; draftGroup: string; setDraftGroup: (v: string) => void; draftInvite: string; setDraftInvite: (v: string) => void; draftChannel: string; setDraftChannel: (v: string) => void; draftMessage: string; setDraftMessage: (v: string) => void; onCreateGroup: () => void; onJoinGroup: () => void; onCreateInvite: () => void; onCreateText: () => void; onCreateVoice: () => void; onSend: () => void }) {
  return <div className="grid gap-4 xl:grid-cols-[0.75fr_1.25fr]"><div className="grid gap-4"><Card><CardHeader><CardTitle>Create or join group</CardTitle><CardDescription>Invite URLs are local command-backed placeholders with honest admission copy.</CardDescription></CardHeader><CardContent className="grid gap-3"><Field label="Group name" value={props.draftGroup} onChange={props.setDraftGroup} /><Button onClick={props.onCreateGroup}><PlusIcon /> Create group</Button><Field label="Invite URL/code" value={props.draftInvite} onChange={props.setDraftInvite} /><Button variant="secondary" onClick={props.onJoinGroup}>Join group</Button>{props.group ? <Button variant="outline" onClick={props.onCreateInvite}>Create invite for {props.group.name}</Button> : null}{props.activeInvite ? <code className="break-all">{props.activeInvite.code}</code> : null}</CardContent></Card><Card><CardHeader><CardTitle>Channels</CardTitle><CardDescription>Retention: {props.retention}</CardDescription></CardHeader><CardContent className="grid gap-3"><Field label="Channel name" value={props.draftChannel} onChange={props.setDraftChannel} /><div className="flex gap-2"><Button onClick={props.onCreateText}>Create text</Button><Button variant="outline" onClick={props.onCreateVoice}>Create voice</Button></div>{props.group?.channels.map((channel) => <Badge key={channel.channel_id} variant={channel.kind === "Voice" ? "secondary" : "outline"}>{channel.kind === "Text" ? channel.name : `🔊 ${channel.name}`}</Badge>)}</CardContent></Card></div><ChatCard title={props.textChannel ? props.textChannel.name : "Create a text channel"} messages={props.messages} draft={props.draftMessage} setDraft={props.setDraftMessage} onSend={props.onSend} /></div>;
}

function VoicePanel({ group, voiceChannels, sessions, activeSession, onJoin, onLeave, onMute, onVolume }: { group: GroupView | null; voiceChannels: AppChannelView[]; sessions: VoiceSessionView[]; activeSession: VoiceSessionView | null; onJoin: (channel: AppChannelView) => void; onLeave: (session: VoiceSessionView) => void; onMute: (session: VoiceSessionView, muted: boolean) => void; onVolume: (session: VoiceSessionView, participant: VoiceParticipantView, volume: number) => void }) {
  return <div className="grid gap-4 xl:grid-cols-[0.8fr_1.2fr]"><Card><CardHeader><CardTitle>Voice channels</CardTitle><CardDescription>{group ? group.name : "Create or join a group first"}</CardDescription></CardHeader><CardContent className="grid gap-2">{voiceChannels.map((channel) => <Button key={channel.channel_id} variant={activeSession?.channel_id === channel.channel_id ? "secondary" : "outline"} onClick={() => onJoin(channel)}><SpeakerLoudIcon /> Join {channel.name}</Button>)}</CardContent></Card><Card><CardHeader><CardTitle>{activeSession ? "Live voice session" : "Not in voice"}</CardTitle><CardDescription>{activeSession?.route ?? "Join a voice channel to see speaking, mute, and volume controls."}</CardDescription></CardHeader><CardContent className="grid gap-4">{activeSession ? <><div className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] p-3"><Label>Mute myself</Label><Switch checked={activeSession.self_muted} onCheckedChange={(checked) => onMute(activeSession, checked)} /></div>{activeSession.participants.map((participant) => <ParticipantVolume key={participant.id} session={activeSession} participant={participant} onVolume={onVolume} />)}<Button variant="destructive" onClick={() => onLeave(activeSession)}><SpeakerOffIcon /> Leave voice</Button></> : <p className="text-sm text-[hsl(var(--muted-foreground))]">Available sessions: {sessions.length}</p>}</CardContent></Card></div>;
}

function ParticipantVolume({ session, participant, onVolume }: { session: VoiceSessionView; participant: VoiceParticipantView; onVolume: (session: VoiceSessionView, participant: VoiceParticipantView, volume: number) => void }) {
  return <div className="rounded-xl border border-[hsl(var(--border))] p-3"><div className="mb-2 flex items-center justify-between"><span className="flex items-center gap-2"><span className={cn("h-2.5 w-2.5 rounded-full", participant.speaking && !participant.muted ? "bg-emerald-300 shadow-[0_0_0_4px_rgba(110,231,183,0.14)]" : participant.muted ? "bg-red-300" : "bg-[hsl(var(--muted))]")} />{participant.name}</span><Badge variant={participant.muted ? "warning" : "secondary"}>{participant.role}</Badge></div><Slider value={[participant.volume]} max={100} step={1} onValueCommit={(value) => onVolume(session, participant, value[0] ?? participant.volume)} /></div>;
}

function ChatCard({ title, messages, draft, setDraft, onSend }: { title: string; messages: MessageView[]; draft: string; setDraft: (v: string) => void; onSend: () => void }) {
  return <Card className="min-h-[32rem]"><CardHeader><CardTitle>{title}</CardTitle><CardDescription>{messages.length} persisted local message(s)</CardDescription></CardHeader><CardContent className="grid h-[28rem] grid-rows-[1fr_auto] gap-3"><ScrollArea className="rounded-xl border border-[hsl(var(--border))] bg-black/10 p-3"><div className="grid gap-3">{messages.length === 0 ? <p className="text-sm text-[hsl(var(--muted-foreground))]">No messages yet.</p> : messages.map((message) => <div key={message.message_id} className="rounded-xl bg-[hsl(var(--secondary)/0.45)] p-3"><div className="mb-1 flex items-center justify-between"><strong>{message.author}</strong><span className="text-xs text-[hsl(var(--muted-foreground))]">{message.sent_at}</span></div><p>{message.body}</p><p className="mt-1 text-xs text-[hsl(var(--muted-foreground))]">{message.status}</p></div>)}</div></ScrollArea><div className="flex gap-2"><Input value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") onSend(); }} placeholder="Message…" /><Button onClick={onSend}>Send</Button></div></CardContent></Card>;
}

function RightRail({ state, activeTheme, activeTemplate }: { state: AppStateView; activeTheme: string; activeTemplate: string }) {
  return <aside className="hidden h-dvh border-l border-[hsl(var(--border))] bg-[hsl(var(--card)/0.48)] p-4 xl:block"><ScrollArea className="h-full"><div className="grid gap-4"><Card className="shadow-none"><CardHeader><CardTitle>Runtime state</CardTitle><CardDescription>{activeTheme} · {activeTemplate}</CardDescription></CardHeader><CardContent className="grid gap-2 text-sm"><Badge variant="success">{state.lifecycle}</Badge><p>{state.groups.length} group(s)</p><p>{state.dms.length} DM(s)</p><p>{state.voice_sessions.filter((s) => s.joined).length} active voice session(s)</p></CardContent></Card><Card className="shadow-none"><CardHeader><CardTitle>Events</CardTitle></CardHeader><CardContent className="grid gap-2">{state.events.slice(0, 8).map((event) => <p key={event.sequence} className="rounded-lg bg-black/15 p-2 text-xs"><strong>{event.kind}</strong><br />{event.summary}</p>)}</CardContent></Card></div></ScrollArea></aside>;
}

function VoiceDock({ session, onLeave, onMute }: { session: VoiceSessionView; onLeave: () => void; onMute: (muted: boolean) => void }) {
  return <div className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--popover)/0.96)] p-3 shadow-2xl"><Badge variant="success">Voice joined</Badge><span className="text-sm">{session.participants.filter((p) => p.speaking && !p.muted).length} speaking</span><Button variant="outline" size="sm" onClick={() => onMute(!session.self_muted)}>{session.self_muted ? <SpeakerOffIcon /> : <SpeakerLoudIcon />} {session.self_muted ? "Unmute" : "Mute"}</Button><Button variant="destructive" size="sm" onClick={onLeave}>Leave</Button></div>;
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return <div className="grid gap-1.5"><Label>{label}</Label><Input value={value} onChange={(event) => onChange(event.target.value)} /></div>;
}

function ConfigSelect({ label, value, options, onChange }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void }) {
  return <div className="flex items-center gap-2 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.35)] px-2 py-1 text-xs text-[hsl(var(--muted-foreground))]"><span>{label}</span><Select value={value} onValueChange={onChange}><SelectTrigger className="h-8 min-w-40 border-0 bg-transparent"><SelectValue /></SelectTrigger><SelectContent>{options.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectContent></Select></div>;
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <p className="mb-2 mt-5 px-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">{children}</p>;
}

function SidebarButton({ children, active, meta, onClick }: { children: React.ReactNode; active?: boolean; meta?: string; onClick?: () => void }) {
  return <Button variant="ghost" onClick={onClick} className={cn("mb-1 h-auto w-full justify-start whitespace-normal rounded-xl px-3 py-2 text-left text-sm text-[hsl(var(--muted-foreground))]", active && "bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]")}><span className="grid gap-0.5"><span className="font-medium">{children}</span>{meta ? <span className="truncate text-[11px] text-[hsl(var(--muted-foreground))]">{meta}</span> : null}</span></Button>;
}

function filterMessages(messages: MessageView[], target: MessageTarget | null): MessageView[] {
  if (!target) return [];
  return messages.filter((message) => targetsEqual(message.target, target));
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
    <div className="flex items-center gap-2 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.35)] px-2 py-1 text-xs text-[hsl(var(--muted-foreground))]">
      <span className="px-1">{label}</span>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger
          aria-label={label}
          className="h-8 min-w-40 border-0 bg-transparent px-2"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function SetupPanel({
  snapshot,
  completedSteps,
  verifyMessage,
  onVerify,
}: {
  snapshot: AppSnapshot;
  completedSteps: number;
  verifyMessage: string | null;
  onVerify: () => void;
}) {
  return (
    <div className="grid gap-4 xl:grid-cols-[1.25fr_0.75fr]">
      <Card className="overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.86)]">
        <CardHeader className="pb-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex gap-4">
              <div className="grid h-14 w-14 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.35)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]">
                <LockClosedIcon className="h-6 w-6" />
              </div>
              <div>
                <CardTitle className="text-2xl">
                  Verify safety numbers
                </CardTitle>
                <CardDescription>
                  Compare the number below with {snapshot.friend.alias} in
                  person or over a trusted call.
                </CardDescription>
                <Button
                  variant="ghost"
                  size="sm"
                  className="mt-1 px-0 text-[hsl(var(--primary))]"
                >
                  How it works <ChevronRightIcon />
                </Button>
              </div>
            </div>
            <Badge variant={snapshot.friend.verified ? "success" : "warning"}>
              Step {Math.min(completedSteps + 1, 4)} of 4
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.62),hsl(var(--card)/0.72))] p-4 shadow-[inset_0_1px_0_hsl(var(--foreground)/0.04)]">
            <div className="flex flex-wrap items-center justify-between gap-3">
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
              <Button variant="outline" size="sm">
                Show number
              </Button>
            </div>
            <div className="mt-4 grid gap-3 rounded-xl border border-[hsl(var(--border))] bg-black/20 p-3 2xl:grid-cols-[1fr_auto] 2xl:items-center">
              <p className="font-mono text-lg font-semibold tracking-[0.12em] text-[hsl(var(--foreground))]">
                {snapshot.friend.safety_number}
              </p>
              <Button onClick={onVerify}>
                {snapshot.friend.verified ? (
                  <CheckCircledIcon />
                ) : (
                  <LockClosedIcon />
                )}{" "}
                Mark as verified
              </Button>
            </div>
            {verifyMessage ? (
              <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                {verifyMessage}
              </p>
            ) : null}
          </div>
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.34)] p-4">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <Avatar>
                  <AvatarFallback>
                    {snapshot.friend.alias.slice(0, 2).toUpperCase()}
                  </AvatarFallback>
                </Avatar>
                <div>
                  <p className="font-semibold">{snapshot.friend.alias}</p>
                  <p className="text-xs text-[hsl(var(--muted-foreground))]">
                    {snapshot.friend.verified
                      ? "Verified just now"
                      : "Awaiting local comparison"}
                  </p>
                </div>
              </div>
              <Badge variant={snapshot.friend.verified ? "success" : "warning"}>
                {snapshot.friend.verified ? "Verified" : "Pending"}
              </Badge>
            </div>
          </div>
          <div>
            <div className="mb-3 flex items-center gap-2">
              <p className="font-semibold">{snapshot.friend.alias}'s devices</p>
              <Badge variant="secondary">{snapshot.devices.length}</Badge>
            </div>
            <div className="grid gap-2">
              {snapshot.devices.map((device) => (
                <div
                  key={device.device_id}
                  className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3"
                >
                  <div className="flex items-center gap-3">
                    <Avatar>
                      <AvatarFallback>
                        {device.device_id.includes("phone") ? "PH" : "LP"}
                      </AvatarFallback>
                    </Avatar>
                    <div>
                      <p className="text-sm font-medium">{device.device_id}</p>
                      <p className="text-xs text-[hsl(var(--muted-foreground))]">
                        leaf {device.leaf_index} ·{" "}
                        {device.local ? "local" : "remote"}
                      </p>
                    </div>
                  </div>
                  <Badge variant={device.authorized ? "success" : "warning"}>
                    {device.authorized ? "Verified just now" : "Blocked"}
                  </Badge>
                </div>
              ))}
            </div>
          </div>
          <InfoRow
            title="Invite details"
            copy={`${snapshot.invite.expires}; ${snapshot.invite.welcome_required}`}
          />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Setup progress</CardTitle>
          <CardDescription>
            {completedSteps}/4 checks complete for this encrypted group.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          <div className="h-2 rounded-full bg-[hsl(var(--muted))]">
            <div
              className="h-full rounded-full bg-[hsl(var(--primary))]"
              style={{ width: `${(completedSteps / 4) * 100}%` }}
            />
          </div>
          {setupChecklist.map((step, index) => (
            <div
              key={step}
              className="flex items-center gap-3 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.42)] p-3"
            >
              <div
                className={cn(
                  "grid h-8 w-8 place-items-center rounded-lg border",
                  index < completedSteps
                    ? "border-emerald-300/40 bg-emerald-300/10 text-emerald-200"
                    : "border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]",
                )}
              >
                {index < completedSteps ? <CheckCircledIcon /> : index + 1}
              </div>
              <span className="text-sm">{step}</span>
            </div>
          ))}
          <Card className="mt-2 border-amber-300/30 bg-amber-300/5 shadow-none">
            <CardContent className="p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              {snapshot.retention.unlimited_warning}
            </CardContent>
          </Card>
        </CardContent>
      </Card>
    </div>
  );
}

function DmPanel({
  dms,
  messages,
  draftDmName,
  setDraftDmName,
  draftMessage,
  setDraftMessage,
  onStartDm,
  onSendDm,
}: {
  dms: { dm_id: string; display_name: string; local_only_copy: string }[];
  messages: { message_id: string; target: { dm_id: string | null }; author: string; body: string; status: string }[];
  draftDmName: string;
  setDraftDmName: (value: string) => void;
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onStartDm: () => void;
  onSendDm: () => void;
}) {
  const activeDm = dms[0] ?? null;
  const visibleMessages = activeDm
    ? messages.filter((message) => message.target.dm_id === activeDm.dm_id)
    : [];
  return (
    <div className="grid gap-4 xl:grid-cols-[0.8fr_1.2fr]">
      <Card>
        <CardHeader>
          <CardTitle>Direct messages</CardTitle>
          <CardDescription>Local harness-backed DM state with no remote delivery claim.</CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">Contact name<Input value={draftDmName} onChange={(event) => setDraftDmName(event.target.value)} /></Label>
          <Button className="mt-4 w-full" onClick={onStartDm}><PlusIcon /> Start/open DM</Button>
          <Separator className="my-4" />
          <Label className="grid gap-2">Message<Input value={draftMessage} onChange={(event) => setDraftMessage(event.target.value)} /></Label>
          <Button variant="secondary" className="mt-3 w-full" disabled={!activeDm} onClick={onSendDm}>Send DM message</Button>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{activeDm ? activeDm.display_name : 'No DM yet'}</CardTitle>
          <CardDescription>{activeDm?.local_only_copy ?? 'Start a DM to create a local conversation.'}</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          {visibleMessages.length === 0 ? <p className="text-sm text-[hsl(var(--muted-foreground))]">No messages yet.</p> : null}
          {visibleMessages.map((message) => (
            <div key={message.message_id} className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
              <div className="flex items-center justify-between gap-3 text-xs text-[hsl(var(--muted-foreground))]"><span>{message.author}</span><span>{message.status}</span></div>
              <p className="mt-1 text-sm">{message.body}</p>
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function JoinPanel({
  snapshot,
  onJoin,
  onCreateInvite,
}: {
  snapshot: AppSnapshot;
  onJoin: () => void;
  onCreateInvite: () => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Join a group</CardTitle>
        <CardDescription>
          Preview the existing invite admission guarantees without adding
          unsupported backend scope.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 lg:grid-cols-2">
        {[
          snapshot.invite.expires,
          snapshot.invite.max_use,
          snapshot.invite.password_gate,
          snapshot.invite.welcome_required,
        ].map((copy) => (
          <div
            key={copy}
            className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]"
          >
            <ChevronRightIcon className="mb-2 text-[hsl(var(--primary))]" />
            {copy}
          </div>
        ))}
        <div className="flex flex-wrap gap-2 lg:col-span-2">
          <Button onClick={onJoin}>Use current invite template</Button>
          <Button variant="outline" onClick={onCreateInvite}>Create copyable invite</Button>
        </div>
      </CardContent>
    </Card>
  );
}

function CreateGroupPanel({
  snapshot,
  onCreate,
}: {
  snapshot: AppSnapshot;
  onCreate: () => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Create a group</CardTitle>
        <CardDescription>
          A polished setup template backed by current governance, invite, and
          retention copy.
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 xl:grid-cols-[0.9fr_1.1fr]">
        <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.42)] p-4">
          <Label className="grid gap-2">
            Group name
            <Input defaultValue="private lab" />
          </Label>
          <Label className="mt-4 grid gap-2">
            Default retention
            <Input defaultValue={snapshot.retention.selected} />
          </Label>
          <Button className="mt-5 w-full" onClick={onCreate}>
            Create local setup
          </Button>
        </div>
        <div className="grid gap-3">
          <InfoRow title="Admission" copy={snapshot.invite.welcome_required} />
          <InfoRow
            title="Retention warning"
            copy={snapshot.retention.unlimited_warning}
          />
          <InfoRow
            title="Metadata posture"
            copy={snapshot.connectivity.metadata_copy}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function ChannelPanel({
  channels,
  messages,
  draftChannel,
  setDraftChannel,
  draftMessage,
  setDraftMessage,
  onCreateChannel,
  onSendMessage,
}: {
  channels: ChannelView[];
  messages: MessageView[];
  draftChannel: string;
  setDraftChannel: (value: string) => void;
  draftMessage: string;
  setDraftMessage: (value: string) => void;
  onCreateChannel: () => void;
  onSendMessage: (channelName: string) => void;
}) {
  const activeChannel = channels[0]?.name ?? '#general';
  const visibleMessages = messages.filter((message) => message.channel === activeChannel);
  return (
    <div className="grid gap-4 xl:grid-cols-[0.8fr_1.2fr]">
      <Card>
        <CardHeader>
          <CardTitle>Create a chat channel</CardTitle>
          <CardDescription>Channel creation is persisted through the AppService command surface.</CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">Channel name<Input value={draftChannel} onChange={(event) => setDraftChannel(event.target.value)} /></Label>
          <Dialog>
            <DialogTrigger asChild><Button className="mt-4 w-full"><PlusIcon /> Create channel</Button></DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Create #{draftChannel.replace(/^#/, '') || 'secure-room'}?</DialogTitle>
                <DialogDescription>This uses the create_channel command and persists through the AppStore boundary.</DialogDescription>
              </DialogHeader>
              <DialogFooter><DialogClose asChild><Button onClick={onCreateChannel}>Confirm local channel</Button></DialogClose></DialogFooter>
            </DialogContent>
          </Dialog>
          <Separator className="my-4" />
          <Label className="grid gap-2">Message<Input value={draftMessage} onChange={(event) => setDraftMessage(event.target.value)} /></Label>
          <Button variant="secondary" className="mt-3 w-full" onClick={() => onSendMessage(activeChannel)}>Send command-backed message</Button>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Channel map</CardTitle>
          <CardDescription>Retention state and local command timeline stay visible at channel level.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          {channels.map((channel) => <InfoRow key={channel.name} title={channel.name} copy={channel.retention_status} />)}
          <Separator />
          {visibleMessages.map((message) => (
            <div key={message.id} className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
              <div className="flex items-center justify-between gap-3 text-xs text-[hsl(var(--muted-foreground))]"><span>{message.author}</span><span>{message.state}</span></div>
              <p className="mt-1 text-sm">{message.body}</p>
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function VoicePanel({
  route,
  participants,
  voiceJoined,
  selfMuted,
  setVoiceJoined,
  setSelfMuted,
  setVolume,
}: {
  route: string;
  participants: VoiceParticipant[];
  voiceJoined: boolean;
  selfMuted: boolean;
  setVoiceJoined: (joined: boolean) => void;
  setSelfMuted: (muted: boolean) => void;
  setVolume: (id: string, value: number[]) => void;
}) {
  return (
    <div className="grid gap-4 xl:grid-cols-[1.1fr_0.9fr]">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div>
              <CardTitle>Voice Lobby</CardTitle>
              <CardDescription>{route}</CardDescription>
            </div>
            <Badge variant={voiceJoined ? "success" : "secondary"}>
              {voiceJoined ? "connected" : "not joined"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3">
          {participants.map((participant) => (
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
                <SpeakerLoudIcon className="text-[hsl(var(--muted-foreground))]" />
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
      <Card>
        <CardHeader>
          <CardTitle>Call controls</CardTitle>
          <CardDescription>
            Mute yourself, join or leave, and tune speaker volume.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-5">
          <ControlRow
            label="Join voice room"
            checked={voiceJoined}
            onCheckedChange={setVoiceJoined}
          />
          <ControlRow
            label="Mute my microphone"
            checked={selfMuted}
            onCheckedChange={setSelfMuted}
          />
          <Button
            variant={voiceJoined ? "destructive" : "default"}
            onClick={() => setVoiceJoined(!voiceJoined)}
          >
            {voiceJoined ? "Leave call" : "Join call"}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function RightRail({
  snapshot,
  participants,
  completedSteps,
  themeLabel,
  templateLabel,
  activityFeed,
}: {
  snapshot: AppSnapshot;
  participants: VoiceParticipant[];
  completedSteps: number;
  themeLabel: string;
  templateLabel: string;
  activityFeed: string[];
}) {
  return (
    <aside className="hidden h-dvh border-l border-[hsl(var(--border))] bg-[hsl(var(--card)/0.58)] backdrop-blur-xl xl:block">
      <Tabs defaultValue="members" className="flex h-full flex-col">
        <div className="border-b border-[hsl(var(--border))] p-4">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="members">Members</TabsTrigger>
            <TabsTrigger value="security">Security</TabsTrigger>
            <TabsTrigger value="activity">Activity</TabsTrigger>
          </TabsList>
        </div>
        <ScrollArea className="min-h-0 flex-1 p-4">
          <TabsContent value="members" className="mt-0 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">
                  In voice — {participants.length}
                </p>
                <h3 className="mt-1 text-lg font-semibold">Speaking now</h3>
              </div>
              <Badge variant="secondary">command-backed</Badge>
            </div>
            {participants.map((participant) => (
              <Card
                key={participant.id}
                className="bg-[hsl(var(--secondary)/0.34)] shadow-none"
              >
                <CardContent className="grid gap-3 p-4">
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <div
                        className={cn(
                          "rounded-full p-1",
                          participant.speaking &&
                            !participant.muted &&
                            "bg-[conic-gradient(from_90deg,rgba(110,231,183,.2),rgba(110,231,183,.9),rgba(110,231,183,.2))]",
                        )}
                      >
                        <Avatar className="h-11 w-11">
                          <AvatarFallback>
                            {participant.name.slice(0, 2).toUpperCase()}
                          </AvatarFallback>
                        </Avatar>
                      </div>
                      <div>
                        <p className="font-medium">
                          {participant.name}
                          {participant.id === "alice" ? " (you)" : ""}
                        </p>
                        <p
                          className={cn(
                            "text-xs",
                            participant.speaking && !participant.muted
                              ? "text-emerald-200"
                              : "text-[hsl(var(--muted-foreground))]",
                          )}
                        >
                          {participant.muted
                            ? "Muted"
                            : participant.speaking
                              ? "Speaking"
                              : "Listening"}
                        </p>
                      </div>
                    </div>
                    <span className="text-xs text-[hsl(var(--muted-foreground))]">
                      {participant.volume}%
                    </span>
                  </div>
                  <div className="grid grid-cols-[44px_1fr] items-center gap-3">
                    <Button variant="outline" size="icon" className="h-9 w-11">
                      <SpeakerLoudIcon />
                    </Button>
                    <Slider
                      value={[participant.volume]}
                      min={0}
                      max={100}
                      step={1}
                    />
                  </div>
                </CardContent>
              </Card>
            ))}
            <Card className="border-amber-300/40 bg-amber-300/5 shadow-none">
              <CardContent className="p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                <LockClosedIcon className="mb-2 text-amber-200" />
                {snapshot.voice_session.status_copy}.{" "}
                {snapshot.security_copy.deletion}.
              </CardContent>
            </Card>
          </TabsContent>
          <TabsContent value="security" className="mt-0 space-y-4">
            <Card>
              <CardHeader>
                <CardTitle>Security posture</CardTitle>
                <CardDescription>
                  {completedSteps}/4 setup checks complete
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                <p>{snapshot.security_copy.metadata}</p>
                <Separator />
                <p>{snapshot.security_copy.deletion}</p>
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <MixerHorizontalIcon /> Theme config
                </CardTitle>
                <CardDescription>
                  {themeLabel} · {templateLabel}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">
                  Theme and template choices are saved through the AppService
                  preference command.
                </p>
              </CardContent>
            </Card>
          </TabsContent>
          <TabsContent value="activity" className="mt-0 space-y-3">
            {activityFeed.map((item) => (
              <p
                key={item}
                className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.4)] p-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]"
              >
                {item}
              </p>
            ))}
          </TabsContent>
        </ScrollArea>
      </Tabs>
    </aside>
  );
}

function VoiceDock({
  route,
  voiceJoined,
  selfMuted,
  setVoiceJoined,
  setSelfMuted,
  participants,
}: {
  route: string;
  voiceJoined: boolean;
  selfMuted: boolean;
  setVoiceJoined: (joined: boolean) => void;
  setSelfMuted: (muted: boolean) => void;
  participants: VoiceParticipant[];
}) {
  const speaking = participants.filter(
    (participant) => participant.speaking && !participant.muted,
  ).length;
  return (
    <div className="fixed bottom-4 left-4 right-4 z-30 grid gap-3 rounded-3xl border border-[hsl(var(--border))] bg-[hsl(var(--popover)/0.95)] p-4 shadow-2xl backdrop-blur-xl md:left-24 md:right-6 xl:right-6 xl:grid-cols-[1.15fr_1.6fr_1.45fr]">
      <Card className="bg-[hsl(var(--secondary)/0.38)] shadow-none">
        <CardContent className="flex items-center justify-between gap-3 p-3">
          <div className="flex items-center gap-3">
            <div className="grid h-10 w-10 place-items-center rounded-xl bg-emerald-300/10 text-emerald-200">
              <MixerHorizontalIcon />
            </div>
            <div>
              <p className="font-medium">
                {voiceJoined
                  ? "Voice session joined"
                  : "Voice session not joined"}
              </p>
              <p className="text-xs text-[hsl(var(--muted-foreground))]">
                {speaking} speaking · command-backed state
              </p>
            </div>
          </div>
          <ChevronRightIcon />
        </CardContent>
      </Card>
      <div className="flex flex-wrap items-center justify-center gap-4">
        <div className="grid gap-1 text-center">
          <Button
            variant={selfMuted ? "destructive" : "outline"}
            size="icon"
            className="h-14 w-14 rounded-full"
            onClick={() => setSelfMuted(!selfMuted)}
          >
            {selfMuted ? <SpeakerOffIcon /> : <PersonIcon />}
          </Button>
          <span className="text-xs text-[hsl(var(--muted-foreground))]">
            Mic
          </span>
        </div>
        <div className="grid gap-1 text-center">
          <Button
            variant="outline"
            size="icon"
            className="h-14 w-14 rounded-full"
          >
            <MixerHorizontalIcon />
          </Button>
          <span className="text-xs text-[hsl(var(--muted-foreground))]">
            Deafen
          </span>
        </div>
        <div className="flex min-w-48 items-center gap-3">
          <div className="grid gap-1 text-center">
            <Button
              variant="outline"
              size="icon"
              className="h-14 w-14 rounded-full"
            >
              <SpeakerLoudIcon />
            </Button>
            <span className="text-xs text-[hsl(var(--muted-foreground))]">
              Speaker
            </span>
          </div>
          <Slider value={[74]} min={0} max={100} step={1} />
        </div>
        <Button
          variant={voiceJoined ? "destructive" : "default"}
          className="h-14 rounded-2xl px-8"
          onClick={() => setVoiceJoined(!voiceJoined)}
        >
          {voiceJoined ? "Leave call" : "Join voice"}
        </Button>
      </div>
      <Card className="bg-[hsl(var(--secondary)/0.38)] shadow-none">
        <CardContent className="grid gap-2 p-3 md:grid-cols-[1fr_auto] md:items-center">
          <div>
            <p className="text-sm font-medium">Relay route</p>
            <p className="text-xs text-[hsl(var(--muted-foreground))]">
              {route}
            </p>
            <div className="mt-2 flex items-center gap-2 text-xs">
              <Badge variant="success">STUN</Badge>
              <span className="h-px w-8 bg-emerald-300/60" />
              <Badge variant="secondary">relay-overlay</Badge>
              <span className="h-px w-8 bg-emerald-300/60" />
              <Badge variant="secondary">TURN</Badge>
            </div>
          </div>
          <div className="rounded-2xl border border-emerald-300/30 bg-emerald-300/10 p-3 text-center text-emerald-200">
            <LockClosedIcon className="mx-auto mb-1" />
            <p className="text-sm font-medium">Secure</p>
            <p className="text-xs">Harness-gated</p>
          </div>
        </CardContent>
      </Card>
    </div>
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

function ControlRow({
  label,
  checked,
  onCheckedChange,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
      <span className="text-sm font-medium">{label}</span>
      <Switch
        aria-label={label}
        checked={checked}
        onCheckedChange={onCheckedChange}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
