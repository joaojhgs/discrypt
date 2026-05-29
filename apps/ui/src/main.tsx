import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  CheckCircledIcon,
  ChevronRightIcon,
  GearIcon,
  LockClosedIcon,
  MixerHorizontalIcon,
  PersonIcon,
  PlusIcon,
  SpeakerLoudIcon,
  SpeakerOffIcon,
} from "@radix-ui/react-icons";
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
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import "./styles.css";

type Workflow = "setup" | "dm" | "join" | "create-group" | "channel" | "voice";

type VoiceParticipant = VoiceParticipantView;
type SetupStepView = {
  label: string;
  complete: boolean;
  detail: string;
};

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
      .catch((error: unknown) => {
        if (mounted) {
          setLoadError(error instanceof Error ? error.message : 'Unable to load app command state');
        }
      });
    return () => {
      mounted = false;
    };
  }, []);

  async function applyCommand(command: Promise<AppState>, success?: (state: AppState) => void) {
    try {
      setCommandError(null);
      const nextState = await command;
      setCommandState(nextState);
      success?.(nextState);
    } catch (error: unknown) {
      setCommandError(error instanceof Error ? error.message : 'Command failed');
    }
  }

  if (loadError) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-red-200">discrypt command surface failed: {loadError}</main>;
  }

  if (!commandState) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">Loading discrypt…</main>;
  }

  const appState = commandState;
  const currentSnapshot = appState.snapshot;
  const activeGroup = appState.active_context?.group_id
    ? appState.groups.find((group) => group.group_id === appState.active_context?.group_id) ?? appState.groups[0] ?? null
    : appState.groups[0] ?? null;
  const activeDmId = appState.active_context?.dm_id ?? appState.dms[0]?.dm_id ?? null;
  const activeDm = activeDmId
    ? appState.dms.find((dm) => dm.dm_id === activeDmId) ?? appState.dms[0] ?? null
    : appState.dms[0] ?? null;
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
  const setupSteps: SetupStepView[] = [
    {
      label: setupChecklist[0],
      complete: currentSnapshot.friend.verified,
      detail: currentSnapshot.friend.verified
        ? "Safety number verified"
        : "Compare the number with Bob before trusting the DM",
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
  const isSetupWorkflow = workflow === "setup";
  const showRightRail = activeTemplate.showRightRail && !isSetupWorkflow;
  const showVoiceDock = !isSetupWorkflow;

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
    const dm = activeDm;
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
          'min-h-dvh bg-[hsl(var(--background))] text-[hsl(var(--foreground))]',
          'grid overflow-hidden',
          showRightRail
            ? 'grid-cols-1 lg:grid-cols-[72px_minmax(250px,320px)_minmax(0,1fr)] xl:grid-cols-[72px_minmax(250px,320px)_minmax(0,1fr)_minmax(280px,340px)]'
            : 'grid-cols-1 lg:grid-cols-[72px_minmax(250px,320px)_minmax(0,1fr)]',
          activeTemplate.density === 'compact' && (showRightRail
            ? 'grid-cols-1 lg:grid-cols-[64px_minmax(230px,290px)_minmax(0,1fr)] xl:grid-cols-[64px_minmax(230px,290px)_minmax(0,1fr)_minmax(260px,310px)]'
            : 'grid-cols-1 lg:grid-cols-[64px_minmax(230px,290px)_minmax(0,1fr)]'),
        )}
      >
        <ServerRail groupLabel={groupLabel} themeLabel={activeTheme.label} />
        <ChannelSidebar
          groupLabel={groupLabel}
          role={activeServer.role}
          textChannels={textChannels}
          voiceChannels={voiceChannels}
          selectedWorkflow={workflow}
          onSelectWorkflow={setWorkflow}
          onOpenCreateGroup={() => setWorkflow('create-group')}
          onOpenJoin={() => setWorkflow('join')}
          onOpenChannel={() => setWorkflow('channel')}
          voiceJoined={voiceJoined}
          participants={participants}
          setupSteps={setupSteps}
          completedSteps={completedSteps}
        />
        <ScrollArea className="h-dvh">
          <section className={cn(
            "min-h-dvh bg-[radial-gradient(circle_at_80%_0%,hsl(var(--primary)/0.10),transparent_34rem)] p-4 md:p-6",
            showVoiceDock ? "pb-52 md:pb-56" : "pb-8",
          )}>
            <TopBar
              groupLabel={groupLabel}
              themeId={asThemeId(activeTheme.id)}
              templateId={asTemplateId(activeTemplate.id)}
              onThemeChange={chooseTheme}
              onTemplateChange={chooseTemplate}
              onOpenCreateGroup={() => setWorkflow('create-group')}
              onOpenJoin={() => setWorkflow('join')}
              onOpenChannel={() => setWorkflow('channel')}
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
                <SetupPanel snapshot={currentSnapshot} setupSteps={setupSteps} completedSteps={completedSteps} verifyMessage={verifyMessage} onVerify={confirmSafetyNumber} />
              </TabsContent>
              <TabsContent value="dm">
                <DmPanel activeDm={activeDm} messages={appState.messages} draftDmName={draftDmName} setDraftDmName={setDraftDmName} draftMessage={draftMessage} setDraftMessage={setDraftMessage} onStartDm={startCommandDm} onSendDm={sendCommandDm} />
              </TabsContent>
              <TabsContent value="join">
                <JoinPanel snapshot={currentSnapshot} onJoin={joinCommandGroup} onCreateInvite={createCommandInvite} />
              </TabsContent>
              <TabsContent value="create-group">
                <CreateGroupPanel snapshot={currentSnapshot} onCreate={createCommandGroup} />
              </TabsContent>
              <TabsContent value="channel">
                <ChannelPanel
                  channels={textChannels}
                  messages={currentSnapshot.messages}
                  draftChannel={draftChannel}
                  setDraftChannel={setDraftChannel}
                  draftMessage={draftMessage}
                  setDraftMessage={setDraftMessage}
                  onCreateChannel={createCommandChannel}
                  onSendMessage={sendCommandMessage}
                />
              </TabsContent>
              <TabsContent value="voice">
                <VoicePanel route={currentSnapshot.voice.route} participants={participants} voiceJoined={voiceJoined} selfMuted={selfMuted} setVoiceJoined={toggleVoiceJoin} setSelfMuted={toggleSelfMute} setVolume={setVolume} />
              </TabsContent>
            </Tabs>
          </section>
        </ScrollArea>
        {showRightRail ? (
          <RightRail snapshot={currentSnapshot} participants={participants} completedSteps={completedSteps} themeLabel={activeTheme.label} templateLabel={activeTemplate.label} activityFeed={currentSnapshot.activity_feed ?? activityFeed} />
        ) : null}
        {showVoiceDock ? (
          <VoiceDock route={currentSnapshot.voice.route} voiceJoined={voiceJoined} selfMuted={selfMuted} setVoiceJoined={toggleVoiceJoin} setSelfMuted={toggleSelfMute} participants={participants} />
        ) : null}
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
      className="min-h-dvh bg-[radial-gradient(circle_at_20%_10%,hsl(var(--primary)/0.12),transparent_24rem),hsl(var(--background))] p-4 text-[hsl(var(--foreground))] md:p-8"
    >
      <div className="mx-auto grid min-h-[calc(100dvh-2rem)] w-full max-w-5xl place-items-center md:min-h-[calc(100dvh-4rem)]">
        <Card className="w-full overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.9)] shadow-2xl shadow-black/30">
          <div className="grid lg:grid-cols-[0.9fr_1.1fr]">
            <CardHeader className="border-b border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.48),transparent)] p-6 lg:border-b-0 lg:border-r lg:p-8">
              <Badge variant="secondary" className="w-fit">first run</Badge>
              <CardTitle className="max-w-md text-3xl leading-tight md:text-4xl">Set up your local discrypt profile</CardTitle>
              <CardDescription className="max-w-md text-base leading-7">
                Create a local identity for this device, or unlock a test-build recovery placeholder. No cloud backup, history restore, QR pairing, or cross-device key recovery is claimed here.
              </CardDescription>
              <div className="grid gap-3 pt-3 text-sm text-[hsl(var(--muted-foreground))]">
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">1. Choose a display name and device label.</div>
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">2. Enter the app shell with command-backed local state.</div>
                <div className="rounded-2xl border border-[hsl(var(--border))] bg-black/10 p-3">3. Verify safety, groups, chat, and voice from the setup checklist.</div>
              </div>
            </CardHeader>
            <CardContent className="grid gap-4 p-6 md:grid-cols-2 lg:p-8">
          {commandError ? <p className="md:col-span-2 rounded-xl border border-red-300/30 bg-red-300/10 p-3 text-sm text-red-100">Command note: {commandError}</p> : null}
          <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
            <div className="mb-4">
              <h2 className="text-lg font-semibold">New local user</h2>
              <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">Best for first machine setup.</p>
            </div>
            <Label className="grid gap-2">Display name<Input value={displayName} onChange={(event) => setDisplayName(event.target.value)} /></Label>
            <Label className="mt-4 grid gap-2">Device name<Input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} /></Label>
            <Button className="mt-auto w-full" onClick={onCreate}>Create new user</Button>
          </div>
          <div className="flex min-h-72 flex-col rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4">
            <div className="mb-4">
              <h2 className="text-lg font-semibold">Existing user</h2>
              <p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">Placeholder recovery for this local build.</p>
            </div>
            <Label className="grid gap-2">Recovery phrase/code<Input value={recoveryCode} onChange={(event) => setRecoveryCode(event.target.value)} /></Label>
            <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
              Local/test-build placeholder only. It unlocks the shell for E2E coverage but does not recover remote devices or message history.
            </p>
            <Button variant="outline" className="mt-auto w-full" onClick={onRecover}>Recover existing user</Button>
          </div>
            </CardContent>
          </div>
        </Card>
      </div>
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
    <aside className="hidden border-r border-[hsl(var(--border))] bg-black/20 p-3 md:flex md:flex-col md:items-center md:gap-3">
      <div className="grid h-10 w-10 place-items-center rounded-2xl bg-[hsl(var(--primary))] font-black text-[hsl(var(--primary-foreground))] shadow-sm">
        d
      </div>
      {[groupLabel, "ops", "dm"].map((name, index) => (
        <Tooltip key={name}>
          <TooltipTrigger asChild>
            <Button
              variant={index === 0 ? "secondary" : "outline"}
              size="icon"
              className={cn(
                "h-11 w-11 rounded-2xl text-xs font-bold text-[hsl(var(--muted-foreground))]",
                index === 0 &&
                  "border-[hsl(var(--primary)/0.5)] text-[hsl(var(--foreground))]",
              )}
            >
              {name.slice(0, 2).toUpperCase()}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">{name}</TooltipContent>
        </Tooltip>
      ))}
      <div
        className="mt-auto grid h-10 w-10 place-items-center rounded-xl border border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]"
        title={themeLabel}
      >
        <GearIcon />
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
  voiceJoined,
  participants,
  setupSteps,
  completedSteps,
}: {
  groupLabel: string;
  role: string;
  textChannels: ChannelView[];
  voiceChannels: ChannelView[];
  selectedWorkflow: Workflow;
  onSelectWorkflow: (workflow: Workflow) => void;
  onOpenCreateGroup: () => void;
  onOpenJoin: () => void;
  onOpenChannel: () => void;
  voiceJoined: boolean;
  participants: VoiceParticipant[];
  setupSteps: SetupStepView[];
  completedSteps: number;
}) {
  const setupTotal = setupSteps.length;
  const setupProgress = setupTotal > 0 ? (completedSteps / setupTotal) * 100 : 0;
  const nextSetupIndex = setupSteps.findIndex((step) => !step.complete);
  return (
    <aside className="hidden h-dvh border-r border-[hsl(var(--border))] bg-[hsl(var(--card)/0.58)] backdrop-blur-xl lg:block">
      <div className="flex h-full flex-col">
        <div className="border-b border-[hsl(var(--border))] p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h1 className="text-lg font-semibold tracking-tight">
                {groupLabel}
              </h1>
              <p className="text-xs text-[hsl(var(--muted-foreground))]">
                {role} · encrypted workspace
              </p>
            </div>
            <Badge variant="success">live</Badge>
          </div>
          <div className="mt-4 grid grid-cols-2 gap-2">
            <Button variant="secondary" size="sm" onClick={onOpenCreateGroup}>
              <PlusIcon /> Create
            </Button>
            <Button variant="outline" size="sm" onClick={onOpenJoin}>
              Join
            </Button>
          </div>
        </div>
        <ScrollArea className="flex-1 p-3">
          <Card className="mb-5 bg-[hsl(var(--secondary)/0.34)] shadow-none">
            <CardHeader className="p-4 pb-2">
              <div className="flex items-center justify-between">
                <CardTitle>Group setup</CardTitle>
                <Badge variant="secondary">{completedSteps} of {setupTotal}</Badge>
              </div>
              <div className="mt-2 h-1.5 rounded-full bg-[hsl(var(--muted))]">
                <div className="h-full rounded-full bg-[hsl(var(--primary))]" style={{ width: `${setupProgress}%` }} />
              </div>
            </CardHeader>
            <CardContent className="grid gap-1 p-3 pt-1">
              {setupSteps.map((step, index) => (
                <Button
                  key={step.label}
                  variant={index === nextSetupIndex ? "outline" : "ghost"}
                  size="sm"
                  className={cn(
                    "h-auto justify-start whitespace-normal py-2 text-left text-xs",
                    index === nextSetupIndex &&
                      "border-[hsl(var(--primary)/0.5)] text-[hsl(var(--foreground))]",
                  )}
                >
                  <span
                    className={cn(
                      "grid h-4 w-4 place-items-center rounded-full border text-[10px]",
                      step.complete
                        ? "border-emerald-300/50 text-emerald-200"
                        : "border-[hsl(var(--primary)/0.65)] text-[hsl(var(--primary))]",
                    )}
                  >
                    {step.complete ? <CheckCircledIcon /> : index + 1}
                  </span>
                  {step.label}
                </Button>
              ))}
            </CardContent>
          </Card>
          <SidebarButton
            active={selectedWorkflow === "setup"}
            onClick={() => onSelectWorkflow("setup")}
          >
            Setup checklist
          </SidebarButton>
          <SectionLabel>Text channels</SectionLabel>
          {textChannels.map((channel) => (
            <SidebarButton
              key={channel.name}
              active={selectedWorkflow === "channel"}
              onClick={onOpenChannel}
              meta={channel.retention_status}
            >
              {channel.name}
            </SidebarButton>
          ))}
          <SectionLabel>Voice rooms</SectionLabel>
          {voiceChannels.map((channel) => (
            <div key={channel.name}>
              <SidebarButton
                active={selectedWorkflow === "voice"}
                onClick={() => onSelectWorkflow("voice")}
                meta={voiceJoined ? "session joined · command-backed" : "ready"}
              >
                {channel.name}
              </SidebarButton>
              <div className="mt-2 grid gap-2 pl-3">
                {participants.map((participant) => (
                  <button
                    key={participant.id}
                    onClick={() => onSelectWorkflow("voice")}
                    className="flex items-center justify-between rounded-lg px-2 py-1.5 text-left text-sm text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--accent))]"
                  >
                    <span className="flex items-center gap-2">
                      <Avatar className="h-7 w-7">
                        <AvatarFallback>
                          {participant.name.slice(0, 2).toUpperCase()}
                        </AvatarFallback>
                      </Avatar>
                      {participant.name}
                    </span>
                    <span
                      className={cn(
                        "h-2.5 w-2.5 rounded-full",
                        participant.speaking && !participant.muted
                          ? "bg-emerald-300 shadow-[0_0_0_4px_rgba(110,231,183,0.14)]"
                          : participant.muted
                            ? "bg-red-300/70"
                            : "bg-[hsl(var(--muted))]",
                      )}
                    />
                  </button>
                ))}
              </div>
            </div>
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
  onOpenChannel,
}: {
  groupLabel: string;
  themeId: ThemeId;
  templateId: TemplateId;
  onThemeChange: (id: ThemeId) => void;
  onTemplateChange: (id: TemplateId) => void;
  onOpenCreateGroup: () => void;
  onOpenJoin: () => void;
  onOpenChannel: () => void;
}) {
  return (
    <Card className="sticky top-4 z-20 border-[hsl(var(--border)/0.8)] bg-[hsl(var(--card)/0.9)] shadow-[0_16px_60px_rgba(2,6,23,0.22)]">
      <div className="flex flex-col gap-3 p-3 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex min-w-0 items-center gap-3">
          <div className="grid h-10 w-10 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.4)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]">
            <LockClosedIcon />
          </div>
          <div className="min-w-0">
            <h2 className="truncate text-xl font-semibold tracking-tight">
              {groupLabel}
            </h2>
            <p className="text-xs text-[hsl(var(--muted-foreground))]">
              End-to-end encrypted · safety numbers enabled
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" size="sm" onClick={onOpenCreateGroup}>
            <PlusIcon /> Create group
          </Button>
          <Button variant="outline" size="sm" onClick={onOpenJoin}>
            <PersonIcon /> Join group
          </Button>
          <Button size="sm" onClick={onOpenChannel}>
            <PlusIcon /> Create channel
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
        </div>
      </div>
    </Card>
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
  const nextStep = setupSteps.find((step) => !step.complete) ?? setupSteps[setupSteps.length - 1];
  const progress = setupTotal > 0 ? (completedSteps / setupTotal) * 100 : 0;

  return (
    <div className="mx-auto grid max-w-6xl gap-5">
      <Card className="overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.88)] shadow-xl shadow-black/20">
        <CardContent className="grid gap-5 p-5 lg:grid-cols-[1fr_auto] lg:items-center lg:p-6">
          <div className="flex min-w-0 gap-4">
            <div className="grid h-14 w-14 shrink-0 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.35)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]">
              <LockClosedIcon className="h-6 w-6" />
            </div>
            <div className="min-w-0">
              <Badge variant="secondary" className="mb-3 w-fit">setup workflow</Badge>
              <h2 className="text-2xl font-semibold tracking-tight md:text-3xl">Finish the local trust setup</h2>
              <p className="mt-2 max-w-3xl text-sm leading-6 text-[hsl(var(--muted-foreground))] md:text-base">
                This screen is the launch checklist for the current local profile: verify Bob, review authorized devices, confirm invite admission, and keep the retention warning visible before chat and voice use.
              </p>
            </div>
          </div>
          <div className="min-w-64 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.36)] p-4">
            <div className="flex items-center justify-between gap-4">
              <span className="text-sm font-medium">Progress</span>
              <Badge variant={completedSteps === setupTotal ? "success" : "warning"}>{completedSteps}/{setupTotal}</Badge>
            </div>
            <div className="mt-3 h-2 rounded-full bg-[hsl(var(--muted))]">
              <div className="h-full rounded-full bg-[hsl(var(--primary))] transition-[width]" style={{ width: `${progress}%` }} />
            </div>
            <p className="mt-3 text-xs leading-5 text-[hsl(var(--muted-foreground))]">
              Next: {nextStep?.label ?? "Ready"}
            </p>
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1.1fr)_minmax(320px,0.9fr)]">
        <Card className="overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.86)]">
          <CardHeader className="pb-3">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <CardTitle className="text-2xl">Verify safety numbers</CardTitle>
                <CardDescription className="max-w-2xl leading-6">
                  Compare this number with {snapshot.friend.alias} in person or over a trusted call. The rest of setup stays visible, but this is the only trust step that is still incomplete by default.
                </CardDescription>
              </div>
              <Badge variant={snapshot.friend.verified ? "success" : "warning"}>
                {snapshot.friend.verified ? "verified" : "needs comparison"}
              </Badge>
            </div>
          </CardHeader>
          <CardContent className="grid gap-4 lg:grid-cols-[0.95fr_1.05fr]">
            <div className="rounded-2xl border border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.62),hsl(var(--card)/0.72))] p-4 shadow-[inset_0_1px_0_hsl(var(--foreground)/0.04)]">
              <div className="flex items-center gap-3">
                <Avatar className="h-12 w-12">
                  <AvatarFallback>{snapshot.friend.alias.slice(0, 2).toUpperCase()}</AvatarFallback>
                </Avatar>
                <div>
                  <p className="text-lg font-semibold">{snapshot.friend.alias}</p>
                  <p className={cn("text-sm", snapshot.friend.verified ? "text-emerald-200" : "text-amber-200")}>
                    {snapshot.friend.verified ? "Verified" : "Unverified"}
                  </p>
                </div>
              </div>
              <div className="mt-4 rounded-xl border border-[hsl(var(--border))] bg-black/20 p-4">
                <p className="break-words font-mono text-lg font-semibold tracking-[0.12em] text-[hsl(var(--foreground))]">
                  {snapshot.friend.safety_number}
                </p>
                <Button className="mt-4 w-full" onClick={onVerify}>
                  {snapshot.friend.verified ? <CheckCircledIcon /> : <LockClosedIcon />} Mark as verified
                </Button>
              </div>
              {verifyMessage ? <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">{verifyMessage}</p> : null}
            </div>

            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-1 2xl:grid-cols-2">
              <InfoRow title="Device review" copy={`${snapshot.devices.length} authorized local device${snapshot.devices.length === 1 ? "" : "s"} available.`} />
              <InfoRow title="Invite admission" copy={snapshot.invite.welcome_required} />
            </div>
          </CardContent>
        </Card>

        <Card className="border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.86)]">
          <CardHeader>
            <CardTitle>Setup checklist</CardTitle>
            <CardDescription>{completedSteps}/{setupTotal} checks complete for this local profile.</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3">
            {setupSteps.map((step, index) => (
              <div key={step.label} className={cn(
                "grid gap-1 rounded-2xl border p-4",
                step.complete
                  ? "border-emerald-300/25 bg-emerald-300/7"
                  : "border-[hsl(var(--primary)/0.45)] bg-[hsl(var(--primary)/0.08)]",
              )}>
                <div className="flex items-center gap-3">
                  <div className={cn(
                    "grid h-9 w-9 shrink-0 place-items-center rounded-xl border text-sm font-semibold",
                    step.complete
                      ? "border-emerald-300/40 bg-emerald-300/10 text-emerald-200"
                      : "border-[hsl(var(--primary)/0.6)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]",
                  )}>
                    {step.complete ? <CheckCircledIcon /> : index + 1}
                  </div>
                  <div className="min-w-0">
                    <p className="font-medium">{step.label}</p>
                    <p className="text-xs leading-5 text-[hsl(var(--muted-foreground))]">{step.detail}</p>
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
  activeDm: { dm_id: string; display_name: string; local_only_copy: string } | null;
  messages: { message_id: string; target: { dm_id: string | null }; author: string; body: string; status: string }[];
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
