import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
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
} from '@radix-ui/react-icons';
import { activityFeed, discryptUiConfig, initialVoiceRoster, setupChecklist, ThemeId, TemplateId } from './app-config';
import { AppSnapshot, ChannelView, loadAppSnapshot, verifySafetyNumber } from './commands';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Slider } from '@/components/ui/slider';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import './styles.css';

type Workflow = 'setup' | 'join' | 'create-group' | 'channel' | 'voice';

type VoiceParticipant = (typeof initialVoiceRoster)[number];

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [themeId, setThemeId] = useState<ThemeId>(discryptUiConfig.activeTheme);
  const [templateId, setTemplateId] = useState<TemplateId>(discryptUiConfig.activeTemplate);
  const [workflow, setWorkflow] = useState<Workflow>('setup');
  const [voiceJoined, setVoiceJoined] = useState(true);
  const [selfMuted, setSelfMuted] = useState(false);
  const [participants, setParticipants] = useState<VoiceParticipant[]>(initialVoiceRoster);
  const [draftChannel, setDraftChannel] = useState('secure-room');
  const [localChannels, setLocalChannels] = useState<ChannelView[]>([]);
  const [groupMode, setGroupMode] = useState<'current' | 'created' | 'joined'>('current');

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

  const activeTheme = discryptUiConfig.themes.find((theme) => theme.id === themeId) ?? discryptUiConfig.themes[0];
  const activeTemplate = discryptUiConfig.templates.find((template) => template.id === templateId) ?? discryptUiConfig.templates[0];
  const themeStyle = activeTheme.cssVars as React.CSSProperties;

  if (loadError) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-red-200">discrypt command surface failed: {loadError}</main>;
  }

  if (!snapshot) {
    return <main className="grid min-h-dvh place-items-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">Loading discrypt…</main>;
  }

  const currentSnapshot = snapshot;
  const activeServer = currentSnapshot.servers[0];
  const channels = [...activeServer.channels, ...localChannels];
  const textChannels = channels.filter((channel) => channel.kind === 'Text');
  const voiceChannels = channels.filter((channel) => channel.kind === 'Voice');
  const groupLabel = groupMode === 'created' ? 'private lab' : groupMode === 'joined' ? 'joined enclave' : activeServer.name;
  const verified = currentSnapshot.friend.verified;
  const completedSteps = [
    verified,
    currentSnapshot.devices.length >= 2,
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
        setSnapshot({ ...currentSnapshot, friend: { ...currentSnapshot.friend, verified: true } });
      }
    } catch (error: unknown) {
      setVerifyMessage(`Safety verification command failed: ${error instanceof Error ? error.message : 'unknown error'}`);
    }
  }

  function createChannel() {
    const name = draftChannel.trim().replace(/^#/, '') || 'secure-room';
    if (!localChannels.some((channel) => channel.name === `#${name}`)) {
      setLocalChannels([...localChannels, { name: `#${name}`, kind: 'Text', retention_status: currentSnapshot.retention.selected }]);
    }
    setWorkflow('channel');
  }

  function setVolume(id: string, value: number[]) {
    setParticipants((current) => current.map((participant) => (participant.id === id ? { ...participant, volume: value[0] } : participant)));
  }

  function toggleSelfMute(checked: boolean) {
    setSelfMuted(checked);
    setParticipants((current) => current.map((participant) => (participant.id === 'alice' ? { ...participant, muted: checked, speaking: !checked } : participant)));
  }

  return (
    <TooltipProvider delayDuration={150}>
      <main
        data-template={activeTemplate.id}
        style={themeStyle}
        className={cn(
          'min-h-dvh bg-[hsl(var(--background))] text-[hsl(var(--foreground))]',
          'grid grid-cols-[72px_minmax(250px,320px)_minmax(0,1fr)_minmax(280px,340px)] overflow-hidden',
          activeTemplate.density === 'compact' && 'grid-cols-[64px_minmax(230px,290px)_minmax(0,1fr)_minmax(260px,310px)]',
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
        />
        <ScrollArea className="h-dvh min-w-0">
          <section className="min-h-dvh bg-[radial-gradient(circle_at_80%_0%,hsl(var(--primary)/0.10),transparent_34rem)] p-4 md:p-6">
            <TopBar
              groupLabel={groupLabel}
              themeId={themeId}
              templateId={templateId}
              onThemeChange={setThemeId}
              onTemplateChange={setTemplateId}
            />
            <Tabs value={workflow} onValueChange={(value) => setWorkflow(value as Workflow)} className="mt-5">
              <TabsList className="flex w-full justify-start overflow-x-auto md:w-auto">
                <TabsTrigger value="setup">Setup</TabsTrigger>
                <TabsTrigger value="join">Join</TabsTrigger>
                <TabsTrigger value="create-group">Create group</TabsTrigger>
                <TabsTrigger value="channel">Channels</TabsTrigger>
                <TabsTrigger value="voice">Voice</TabsTrigger>
              </TabsList>

              <TabsContent value="setup">
                <SetupPanel
                  snapshot={snapshot}
                  completedSteps={completedSteps}
                  verifyMessage={verifyMessage}
                  onVerify={confirmSafetyNumber}
                />
              </TabsContent>
              <TabsContent value="join">
                <JoinPanel snapshot={snapshot} onJoin={() => { setGroupMode('joined'); setWorkflow('setup'); }} />
              </TabsContent>
              <TabsContent value="create-group">
                <CreateGroupPanel snapshot={snapshot} onCreate={() => { setGroupMode('created'); setWorkflow('channel'); }} />
              </TabsContent>
              <TabsContent value="channel">
                <ChannelPanel
                  channels={textChannels}
                  draftChannel={draftChannel}
                  setDraftChannel={setDraftChannel}
                  onCreateChannel={createChannel}
                />
              </TabsContent>
              <TabsContent value="voice">
                <VoicePanel
                  route={snapshot.voice.route}
                  participants={participants}
                  voiceJoined={voiceJoined}
                  selfMuted={selfMuted}
                  setVoiceJoined={setVoiceJoined}
                  setSelfMuted={toggleSelfMute}
                  setVolume={setVolume}
                />
              </TabsContent>
            </Tabs>
          </section>
        </ScrollArea>
        <RightRail
          snapshot={snapshot}
          participants={participants}
          completedSteps={completedSteps}
          themeLabel={activeTheme.label}
          templateLabel={activeTemplate.label}
        />
        <VoiceDock
          route={snapshot.voice.route}
          voiceJoined={voiceJoined}
          selfMuted={selfMuted}
          setVoiceJoined={setVoiceJoined}
          setSelfMuted={toggleSelfMute}
          participants={participants}
        />
      </main>
    </TooltipProvider>
  );
}

function ServerRail({ groupLabel, themeLabel }: { groupLabel: string; themeLabel: string }) {
  return (
    <aside className="hidden border-r border-[hsl(var(--border))] bg-black/20 p-3 md:flex md:flex-col md:items-center md:gap-3">
      <div className="grid h-10 w-10 place-items-center rounded-2xl bg-[hsl(var(--primary))] font-black text-[hsl(var(--primary-foreground))] shadow-sm">d</div>
      {[groupLabel, 'ops', 'dm'].map((name, index) => (
        <Tooltip key={name}>
          <TooltipTrigger asChild>
            <Button variant={index === 0 ? 'secondary' : 'outline'} size="icon" className={cn('h-11 w-11 rounded-2xl text-xs font-bold text-[hsl(var(--muted-foreground))]', index === 0 && 'border-[hsl(var(--primary)/0.5)] text-[hsl(var(--foreground))]')}>
              {name.slice(0, 2).toUpperCase()}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">{name}</TooltipContent>
        </Tooltip>
      ))}
      <div className="mt-auto grid h-10 w-10 place-items-center rounded-xl border border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]" title={themeLabel}>
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
}) {
  return (
    <aside className="hidden h-dvh border-r border-[hsl(var(--border))] bg-[hsl(var(--card)/0.58)] backdrop-blur-xl lg:block">
      <div className="flex h-full flex-col">
        <div className="border-b border-[hsl(var(--border))] p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h1 className="text-lg font-semibold tracking-tight">{groupLabel}</h1>
              <p className="text-xs text-[hsl(var(--muted-foreground))]">{role} · encrypted workspace</p>
            </div>
            <Badge variant="success">live</Badge>
          </div>
          <div className="mt-4 grid grid-cols-2 gap-2">
            <Button variant="secondary" size="sm" onClick={onOpenCreateGroup}><PlusIcon /> Create</Button>
            <Button variant="outline" size="sm" onClick={onOpenJoin}>Join</Button>
          </div>
        </div>
        <ScrollArea className="flex-1 p-3">
          <Card className="mb-5 bg-[hsl(var(--secondary)/0.34)] shadow-none">
            <CardHeader className="p-4 pb-2">
              <div className="flex items-center justify-between">
                <CardTitle>Group setup</CardTitle>
                <Badge variant="secondary">4 of 5</Badge>
              </div>
              <div className="mt-2 h-1.5 rounded-full bg-[hsl(var(--muted))]"><div className="h-full w-4/5 rounded-full bg-[hsl(var(--primary))]" /></div>
            </CardHeader>
            <CardContent className="grid gap-1 p-3 pt-1">
              {setupChecklist.slice(0, 4).map((step, index) => (
                <Button key={step} variant={index === 2 ? 'outline' : 'ghost'} size="sm" className={cn('h-auto justify-start whitespace-normal py-2 text-left text-xs', index === 2 && 'border-[hsl(var(--primary)/0.5)] text-[hsl(var(--foreground))]')}>
                  <span className={cn('grid h-4 w-4 place-items-center rounded-full border text-[10px]', index === 2 ? 'border-[hsl(var(--primary))]' : 'border-emerald-300/50 text-emerald-200')}>{index === 2 ? '' : <CheckCircledIcon />}</span>
                  {step}
                </Button>
              ))}
            </CardContent>
          </Card>
          <SidebarButton active={selectedWorkflow === 'setup'} onClick={() => onSelectWorkflow('setup')}>Setup checklist</SidebarButton>
          <SectionLabel>Text channels</SectionLabel>
          {textChannels.map((channel) => (
            <SidebarButton key={channel.name} active={selectedWorkflow === 'channel'} onClick={onOpenChannel} meta={channel.retention_status}>
              {channel.name}
            </SidebarButton>
          ))}
          <SectionLabel>Voice rooms</SectionLabel>
          {voiceChannels.map((channel) => (
            <div key={channel.name}>
              <SidebarButton active={selectedWorkflow === 'voice'} onClick={() => onSelectWorkflow('voice')} meta={voiceJoined ? 'connected · 2 speaking' : 'ready'}>
                {channel.name}
              </SidebarButton>
              <div className="mt-2 grid gap-2 pl-3">
                {participants.map((participant) => (
                  <button key={participant.id} onClick={() => onSelectWorkflow('voice')} className="flex items-center justify-between rounded-lg px-2 py-1.5 text-left text-sm text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--accent))]">
                    <span className="flex items-center gap-2"><Avatar className="h-7 w-7"><AvatarFallback>{participant.name.slice(0, 2).toUpperCase()}</AvatarFallback></Avatar>{participant.name}</span>
                    <span className={cn('h-2.5 w-2.5 rounded-full', participant.speaking && !participant.muted ? 'bg-emerald-300 shadow-[0_0_0_4px_rgba(110,231,183,0.14)]' : participant.muted ? 'bg-red-300/70' : 'bg-[hsl(var(--muted))]')} />
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
  return <p className="mb-2 mt-5 px-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">{children}</p>;
}

function SidebarButton({ children, active, meta, onClick }: { children: React.ReactNode; active?: boolean; meta?: string; onClick?: () => void }) {
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      className={cn('mb-1 h-auto w-full justify-start whitespace-normal rounded-xl px-3 py-2 text-left text-sm text-[hsl(var(--muted-foreground))]', active && 'bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]')}
    >
      <span className="grid gap-0.5">
        <span className="font-medium">{children}</span>
        {meta ? <span className="truncate text-[11px] text-[hsl(var(--muted-foreground))]">{meta}</span> : null}
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
}: {
  groupLabel: string;
  themeId: ThemeId;
  templateId: TemplateId;
  onThemeChange: (id: ThemeId) => void;
  onTemplateChange: (id: TemplateId) => void;
}) {
  return (
    <Card className="sticky top-4 z-20 border-[hsl(var(--border)/0.8)] bg-[hsl(var(--card)/0.9)] shadow-[0_16px_60px_rgba(2,6,23,0.22)]">
      <div className="flex flex-col gap-3 p-3 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex min-w-0 items-center gap-3">
          <div className="grid h-10 w-10 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.4)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]"><LockClosedIcon /></div>
          <div className="min-w-0">
            <h2 className="truncate text-xl font-semibold tracking-tight">{groupLabel}</h2>
            <p className="text-xs text-[hsl(var(--muted-foreground))]">End-to-end encrypted · safety numbers enabled</p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" size="sm"><PlusIcon /> Create group</Button>
          <Button variant="outline" size="sm"><PersonIcon /> Join group</Button>
          <Button size="sm"><PlusIcon /> Create channel</Button>
          <ConfigSelect label="Theme" value={themeId} onChange={(value) => onThemeChange(value as ThemeId)} options={discryptUiConfig.themes.map((theme) => ({ value: theme.id, label: theme.label }))} />
          <ConfigSelect label="Template" value={templateId} onChange={(value) => onTemplateChange(value as TemplateId)} options={discryptUiConfig.templates.map((template) => ({ value: template.id, label: template.label }))} />
        </div>
      </div>
    </Card>
  );
}

function ConfigSelect({ label, value, options, onChange }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void }) {
  return (
    <div className="flex items-center gap-2 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.35)] px-2 py-1 text-xs text-[hsl(var(--muted-foreground))]">
      <span className="px-1">{label}</span>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger aria-label={label} className="h-8 min-w-40 border-0 bg-transparent px-2">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}
        </SelectContent>
      </Select>
    </div>
  );
}

function SetupPanel({ snapshot, completedSteps, verifyMessage, onVerify }: { snapshot: AppSnapshot; completedSteps: number; verifyMessage: string | null; onVerify: () => void }) {
  return (
    <div className="grid gap-4 xl:grid-cols-[1.25fr_0.75fr]">
      <Card className="overflow-hidden border-[hsl(var(--border)/0.9)] bg-[hsl(var(--card)/0.86)]">
        <CardHeader className="pb-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex gap-4">
              <div className="grid h-14 w-14 place-items-center rounded-2xl border border-[hsl(var(--primary)/0.35)] bg-[hsl(var(--primary)/0.12)] text-[hsl(var(--primary))]"><LockClosedIcon className="h-6 w-6" /></div>
              <div>
                <CardTitle className="text-2xl">Verify safety numbers</CardTitle>
                <CardDescription>Compare the number below with {snapshot.friend.alias} in person or over a trusted call.</CardDescription>
                <Button variant="ghost" size="sm" className="mt-1 px-0 text-[hsl(var(--primary))]">How it works <ChevronRightIcon /></Button>
              </div>
            </div>
            <Badge variant={snapshot.friend.verified ? 'success' : 'warning'}>Step {Math.min(completedSteps + 1, 4)} of 4</Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[linear-gradient(135deg,hsl(var(--secondary)/0.62),hsl(var(--card)/0.72))] p-4 shadow-[inset_0_1px_0_hsl(var(--foreground)/0.04)]">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <Avatar className="h-12 w-12"><AvatarFallback>{snapshot.friend.alias.slice(0, 2).toUpperCase()}</AvatarFallback></Avatar>
                <div><p className="text-lg font-semibold">{snapshot.friend.alias}</p><p className={cn('text-sm', snapshot.friend.verified ? 'text-emerald-200' : 'text-amber-200')}>{snapshot.friend.verified ? 'Verified' : 'Unverified'}</p></div>
              </div>
              <Button variant="outline" size="sm">Show number</Button>
            </div>
            <div className="mt-4 grid gap-3 rounded-xl border border-[hsl(var(--border))] bg-black/20 p-3 2xl:grid-cols-[1fr_auto] 2xl:items-center">
              <p className="font-mono text-lg font-semibold tracking-[0.12em] text-[hsl(var(--foreground))]">{snapshot.friend.safety_number}</p>
              <Button onClick={onVerify}>{snapshot.friend.verified ? <CheckCircledIcon /> : <LockClosedIcon />} Mark as verified</Button>
            </div>
            {verifyMessage ? <p className="mt-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">{verifyMessage}</p> : null}
          </div>
          <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.34)] p-4">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <Avatar><AvatarFallback>{snapshot.friend.alias.slice(0, 2).toUpperCase()}</AvatarFallback></Avatar>
                <div><p className="font-semibold">{snapshot.friend.alias}</p><p className="text-xs text-[hsl(var(--muted-foreground))]">{snapshot.friend.verified ? 'Verified just now' : 'Awaiting local comparison'}</p></div>
              </div>
              <Badge variant={snapshot.friend.verified ? 'success' : 'warning'}>{snapshot.friend.verified ? 'Verified' : 'Pending'}</Badge>
            </div>
          </div>
          <div>
            <div className="mb-3 flex items-center gap-2"><p className="font-semibold">{snapshot.friend.alias}'s devices</p><Badge variant="secondary">{snapshot.devices.length}</Badge></div>
            <div className="grid gap-2">
              {snapshot.devices.map((device) => (
                <div key={device.device_id} className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3">
                  <div className="flex items-center gap-3"><Avatar><AvatarFallback>{device.device_id.includes('phone') ? 'PH' : 'LP'}</AvatarFallback></Avatar><div><p className="text-sm font-medium">{device.device_id}</p><p className="text-xs text-[hsl(var(--muted-foreground))]">leaf {device.leaf_index} · {device.local ? 'local' : 'remote'}</p></div></div>
                  <Badge variant={device.authorized ? 'success' : 'warning'}>{device.authorized ? 'Verified just now' : 'Blocked'}</Badge>
                </div>
              ))}
            </div>
          </div>
          <InfoRow title="Invite details" copy={`${snapshot.invite.expires}; ${snapshot.invite.welcome_required}`} />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Setup progress</CardTitle>
          <CardDescription>{completedSteps}/4 checks complete for this encrypted group.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          <div className="h-2 rounded-full bg-[hsl(var(--muted))]"><div className="h-full rounded-full bg-[hsl(var(--primary))]" style={{ width: `${(completedSteps / 4) * 100}%` }} /></div>
          {setupChecklist.map((step, index) => (
            <div key={step} className="flex items-center gap-3 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.42)] p-3">
              <div className={cn('grid h-8 w-8 place-items-center rounded-lg border', index < completedSteps ? 'border-emerald-300/40 bg-emerald-300/10 text-emerald-200' : 'border-[hsl(var(--border))] text-[hsl(var(--muted-foreground))]')}>{index < completedSteps ? <CheckCircledIcon /> : index + 1}</div>
              <span className="text-sm">{step}</span>
            </div>
          ))}
          <Card className="mt-2 border-amber-300/30 bg-amber-300/5 shadow-none"><CardContent className="p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]">{snapshot.retention.unlimited_warning}</CardContent></Card>
        </CardContent>
      </Card>
    </div>
  );
}

function JoinPanel({ snapshot, onJoin }: { snapshot: AppSnapshot; onJoin: () => void }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Join a group</CardTitle>
        <CardDescription>Preview the existing invite admission guarantees without adding unsupported backend scope.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 lg:grid-cols-2">
        {[snapshot.invite.expires, snapshot.invite.max_use, snapshot.invite.password_gate, snapshot.invite.welcome_required].map((copy) => (
          <div key={copy} className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]">
            <ChevronRightIcon className="mb-2 text-[hsl(var(--primary))]" />{copy}
          </div>
        ))}
        <div className="lg:col-span-2"><Button onClick={onJoin}>Use current invite template</Button></div>
      </CardContent>
    </Card>
  );
}

function CreateGroupPanel({ snapshot, onCreate }: { snapshot: AppSnapshot; onCreate: () => void }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Create a group</CardTitle>
        <CardDescription>A polished setup template backed by current governance, invite, and retention copy.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 xl:grid-cols-[0.9fr_1.1fr]">
        <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.42)] p-4">
          <Label className="grid gap-2">Group name<Input defaultValue="private lab" /></Label>
          <Label className="mt-4 grid gap-2">Default retention<Input defaultValue={snapshot.retention.selected} /></Label>
          <Button className="mt-5 w-full" onClick={onCreate}>Create local setup</Button>
        </div>
        <div className="grid gap-3">
          <InfoRow title="Admission" copy={snapshot.invite.welcome_required} />
          <InfoRow title="Retention warning" copy={snapshot.retention.unlimited_warning} />
          <InfoRow title="Metadata posture" copy={snapshot.connectivity.metadata_copy} />
        </div>
      </CardContent>
    </Card>
  );
}

function ChannelPanel({ channels, draftChannel, setDraftChannel, onCreateChannel }: { channels: ChannelView[]; draftChannel: string; setDraftChannel: (value: string) => void; onCreateChannel: () => void }) {
  return (
    <div className="grid gap-4 xl:grid-cols-[0.8fr_1.2fr]">
      <Card>
        <CardHeader>
          <CardTitle>Create a chat channel</CardTitle>
          <CardDescription>Local UI template for the current channel model.</CardDescription>
        </CardHeader>
        <CardContent>
          <Label className="grid gap-2">Channel name<Input value={draftChannel} onChange={(event) => setDraftChannel(event.target.value)} /></Label>
          <Dialog>
            <DialogTrigger asChild><Button className="mt-4 w-full"><PlusIcon /> Create channel</Button></DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Create #{draftChannel.replace(/^#/, '') || 'secure-room'}?</DialogTitle>
                <DialogDescription>This updates the local shell state only; backend channel persistence is intentionally outside this UI polish pass.</DialogDescription>
              </DialogHeader>
              <DialogFooter><DialogClose asChild><Button onClick={onCreateChannel}>Confirm local channel</Button></DialogClose></DialogFooter>
            </DialogContent>
          </Dialog>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Channel map</CardTitle>
          <CardDescription>Retention state stays visible at channel level.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          {channels.map((channel) => <InfoRow key={channel.name} title={channel.name} copy={channel.retention_status} />)}
        </CardContent>
      </Card>
    </div>
  );
}

function VoicePanel({ route, participants, voiceJoined, selfMuted, setVoiceJoined, setSelfMuted, setVolume }: { route: string; participants: VoiceParticipant[]; voiceJoined: boolean; selfMuted: boolean; setVoiceJoined: (joined: boolean) => void; setSelfMuted: (muted: boolean) => void; setVolume: (id: string, value: number[]) => void }) {
  return (
    <div className="grid gap-4 xl:grid-cols-[1.1fr_0.9fr]">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div><CardTitle>Voice Lobby</CardTitle><CardDescription>{route}</CardDescription></div>
            <Badge variant={voiceJoined ? 'success' : 'secondary'}>{voiceJoined ? 'connected' : 'not joined'}</Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3">
          {participants.map((participant) => (
            <div key={participant.id} className="grid gap-3 rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4 md:grid-cols-[1fr_180px] md:items-center">
              <div className="flex items-center gap-3">
                <div className={cn('rounded-2xl p-0.5', participant.speaking && !participant.muted && 'bg-emerald-300/70')}><Avatar><AvatarFallback>{participant.name.slice(0, 2).toUpperCase()}</AvatarFallback></Avatar></div>
                <div>
                  <p className="font-medium">{participant.name} <span className="text-xs text-[hsl(var(--muted-foreground))]">· {participant.role}</span></p>
                  <p className="text-xs text-[hsl(var(--muted-foreground))]">{participant.muted ? 'muted' : participant.speaking ? 'speaking now' : 'listening'}</p>
                </div>
              </div>
              <div className="flex items-center gap-3"><SpeakerLoudIcon className="text-[hsl(var(--muted-foreground))]" /><Slider value={[participant.volume]} min={0} max={100} step={1} onValueChange={(value) => setVolume(participant.id, value)} /></div>
            </div>
          ))}
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle>Call controls</CardTitle><CardDescription>Mute yourself, join or leave, and tune speaker volume.</CardDescription></CardHeader>
        <CardContent className="grid gap-5">
          <ControlRow label="Join voice room" checked={voiceJoined} onCheckedChange={setVoiceJoined} />
          <ControlRow label="Mute my microphone" checked={selfMuted} onCheckedChange={setSelfMuted} />
          <Button variant={voiceJoined ? 'destructive' : 'default'} onClick={() => setVoiceJoined(!voiceJoined)}>{voiceJoined ? 'Leave call' : 'Join call'}</Button>
        </CardContent>
      </Card>
    </div>
  );
}

function RightRail({ snapshot, participants, completedSteps, themeLabel, templateLabel }: { snapshot: AppSnapshot; participants: VoiceParticipant[]; completedSteps: number; themeLabel: string; templateLabel: string }) {
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
                <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-[hsl(var(--muted-foreground))]">In voice — {participants.length}</p>
                <h3 className="mt-1 text-lg font-semibold">Speaking now</h3>
              </div>
              <ControlRow label="Self mute" checked={false} onCheckedChange={() => undefined} />
            </div>
            {participants.map((participant) => (
              <Card key={participant.id} className="bg-[hsl(var(--secondary)/0.34)] shadow-none">
                <CardContent className="grid gap-3 p-4">
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <div className={cn('rounded-full p-1', participant.speaking && !participant.muted && 'bg-[conic-gradient(from_90deg,rgba(110,231,183,.2),rgba(110,231,183,.9),rgba(110,231,183,.2))]')}>
                        <Avatar className="h-11 w-11"><AvatarFallback>{participant.name.slice(0, 2).toUpperCase()}</AvatarFallback></Avatar>
                      </div>
                      <div>
                        <p className="font-medium">{participant.name}{participant.id === 'alice' ? ' (you)' : ''}</p>
                        <p className={cn('text-xs', participant.speaking && !participant.muted ? 'text-emerald-200' : 'text-[hsl(var(--muted-foreground))]')}>{participant.muted ? 'Muted' : participant.speaking ? 'Speaking' : 'Listening'}</p>
                      </div>
                    </div>
                    <span className="text-xs text-[hsl(var(--muted-foreground))]">{participant.volume}%</span>
                  </div>
                  <div className="grid grid-cols-[44px_1fr] items-center gap-3">
                    <Button variant="outline" size="icon" className="h-9 w-11"><SpeakerLoudIcon /></Button>
                    <Slider value={[participant.volume]} min={0} max={100} step={1} />
                  </div>
                </CardContent>
              </Card>
            ))}
            <Card className="border-amber-300/40 bg-amber-300/5 shadow-none">
              <CardContent className="p-4 text-sm leading-6 text-[hsl(var(--muted-foreground))]"><LockClosedIcon className="mb-2 text-amber-200" />Voice and metadata are encrypted. Deleted after {snapshot.retention.selected} across relays.</CardContent>
            </Card>
          </TabsContent>
          <TabsContent value="security" className="mt-0 space-y-4">
            <Card><CardHeader><CardTitle>Security posture</CardTitle><CardDescription>{completedSteps}/4 setup checks complete</CardDescription></CardHeader><CardContent className="space-y-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]"><p>{snapshot.security_copy.metadata}</p><Separator /><p>{snapshot.security_copy.deletion}</p></CardContent></Card>
            <Card><CardHeader><CardTitle className="flex items-center gap-2"><MixerHorizontalIcon /> Theme config</CardTitle><CardDescription>{themeLabel} · {templateLabel}</CardDescription></CardHeader><CardContent><p className="text-sm leading-6 text-[hsl(var(--muted-foreground))]">Edit <code>src/app-config.ts</code> to change default theme, template, density, radius, copy presets, and CSS variables.</p></CardContent></Card>
          </TabsContent>
          <TabsContent value="activity" className="mt-0 space-y-3">
            {activityFeed.map((item) => <p key={item} className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.4)] p-3 text-sm leading-6 text-[hsl(var(--muted-foreground))]">{item}</p>)}
          </TabsContent>
        </ScrollArea>
      </Tabs>
    </aside>
  );
}

function VoiceDock({ route, voiceJoined, selfMuted, setVoiceJoined, setSelfMuted, participants }: { route: string; voiceJoined: boolean; selfMuted: boolean; setVoiceJoined: (joined: boolean) => void; setSelfMuted: (muted: boolean) => void; participants: VoiceParticipant[] }) {
  const speaking = participants.filter((participant) => participant.speaking && !participant.muted).length;
  return (
    <div className="fixed bottom-4 left-4 right-4 z-30 grid gap-3 rounded-3xl border border-[hsl(var(--border))] bg-[hsl(var(--popover)/0.95)] p-4 shadow-2xl backdrop-blur-xl md:left-24 md:right-6 xl:right-6 xl:grid-cols-[1.15fr_1.6fr_1.45fr]">
      <Card className="bg-[hsl(var(--secondary)/0.38)] shadow-none">
        <CardContent className="flex items-center justify-between gap-3 p-3">
          <div className="flex items-center gap-3">
            <div className="grid h-10 w-10 place-items-center rounded-xl bg-emerald-300/10 text-emerald-200"><MixerHorizontalIcon /></div>
            <div><p className="font-medium">Connected to Voice Lobby</p><p className="text-xs text-[hsl(var(--muted-foreground))]">{speaking} speaking · 00:12:47</p></div>
          </div>
          <ChevronRightIcon />
        </CardContent>
      </Card>
      <div className="flex flex-wrap items-center justify-center gap-4">
        <div className="grid gap-1 text-center"><Button variant={selfMuted ? 'destructive' : 'outline'} size="icon" className="h-14 w-14 rounded-full" onClick={() => setSelfMuted(!selfMuted)}>{selfMuted ? <SpeakerOffIcon /> : <PersonIcon />}</Button><span className="text-xs text-[hsl(var(--muted-foreground))]">Mic</span></div>
        <div className="grid gap-1 text-center"><Button variant="outline" size="icon" className="h-14 w-14 rounded-full"><MixerHorizontalIcon /></Button><span className="text-xs text-[hsl(var(--muted-foreground))]">Deafen</span></div>
        <div className="flex min-w-48 items-center gap-3"><div className="grid gap-1 text-center"><Button variant="outline" size="icon" className="h-14 w-14 rounded-full"><SpeakerLoudIcon /></Button><span className="text-xs text-[hsl(var(--muted-foreground))]">Speaker</span></div><Slider value={[74]} min={0} max={100} step={1} /></div>
        <Button variant={voiceJoined ? 'destructive' : 'default'} className="h-14 rounded-2xl px-8" onClick={() => setVoiceJoined(!voiceJoined)}>{voiceJoined ? 'Leave call' : 'Join voice'}</Button>
      </div>
      <Card className="bg-[hsl(var(--secondary)/0.38)] shadow-none">
        <CardContent className="grid gap-2 p-3 md:grid-cols-[1fr_auto] md:items-center">
          <div><p className="text-sm font-medium">Relay route</p><p className="text-xs text-[hsl(var(--muted-foreground))]">{route}</p><div className="mt-2 flex items-center gap-2 text-xs"><Badge variant="success">STUN</Badge><span className="h-px w-8 bg-emerald-300/60" /><Badge variant="secondary">relay-overlay</Badge><span className="h-px w-8 bg-emerald-300/60" /><Badge variant="secondary">TURN</Badge></div></div>
          <div className="rounded-2xl border border-emerald-300/30 bg-emerald-300/10 p-3 text-center text-emerald-200"><LockClosedIcon className="mx-auto mb-1" /><p className="text-sm font-medium">Secure</p><p className="text-xs">Relay active</p></div>
        </CardContent>
      </Card>
    </div>
  );
}

function InfoRow({ title, copy }: { title: string; copy: string }) {
  return <div className="rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-4"><p className="font-medium">{title}</p><p className="mt-1 text-sm leading-6 text-[hsl(var(--muted-foreground))]">{copy}</p></div>;
}

function ControlRow({ label, checked, onCheckedChange }: { label: string; checked: boolean; onCheckedChange: (checked: boolean) => void }) {
  return <div className="flex items-center justify-between rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.38)] p-3"><span className="text-sm font-medium">{label}</span><Switch aria-label={label} checked={checked} onCheckedChange={onCheckedChange} /></div>;
}

createRoot(document.getElementById('root')!).render(<App />);
