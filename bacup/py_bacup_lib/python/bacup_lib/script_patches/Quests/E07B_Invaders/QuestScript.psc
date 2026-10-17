; FO76's server ran this event. Phase N (stages 200 / 280 / 320) runs its endless alien wave (EncounterWaves row
; N-1) until its siphon is destroyed; aliens killed during the phase count toward the boss spawn. A siphon stays
; shielded until its boss stage (260 / 314 / 600). Kills are counted here: the vanilla FO4 DefaultCounterQuestA-C
; have no preReqStage, so they would count all three phases at once.

Function StartEvent()
    Quest owner = Self as Quest
    EWS = owner as DefaultQuestEncounterWaveScript
    DEQ = owner as DefaultEventQuest
    ProbeScript = Alias_Activators_AllProbes as Quests:E07B_Invaders:ExtractorScript
    If EWS != None
        RegisterForCustomEvent(EWS, "WaveComplete")
    EndIf
    SetKills("A", 0)
    SetKills("B", 0)
    SetKills("C", 0)
    Int required = KillsRequired()
    SetDisplayVariable("AlienKillsRequiredA", required)
    SetDisplayVariable("AlienKillsRequiredB", required)
    SetDisplayVariable("AlienKillsRequiredC", required)
    If Alias_Activators_AllProbes != None
        Int index = 0
        While index < Alias_Activators_AllProbes.GetCount()
            ObjectReference probe = Alias_Activators_AllProbes.GetAt(index)
            If probe != None
                probe.ClearDestruction()
                If ProbeScript != None
                    ProbeScript.SetShield(probe, True)
                EndIf
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Function BeginPhase(Int aiPhase)
    If EWS != None
        EWS.StartEncounterWave(aiPhase - 1)
    EndIf
    StartTimer(2.0, 3)
    StartTimer(TimeBetweenExtractorPulses, 1)
    StartTimer(TimeBetweenMissileStrikes, 2)
EndFunction

Function DropShield(ReferenceAlias akSiphon)
    If ProbeScript != None && akSiphon != None && akSiphon.GetReference() != None
        ProbeScript.SetShield(akSiphon.GetReference(), False)
    EndIf
EndFunction

Bool Function IsShielded(ObjectReference akSiphon)
    If akSiphon == SiphonRef(4)
        Return !IsStageDone(260)
    ElseIf akSiphon == SiphonRef(19)
        Return !IsStageDone(314)
    ElseIf akSiphon == SiphonRef(20)
        Return !IsStageDone(600)
    EndIf
    Return False
EndFunction

; Stage 270 / 315 ask the players to regroup at the next siphon.
Function WatchArrival(Int aiSiphonAlias)
    ObjectReference siphon = SiphonRef(aiSiphonAlias)
    If siphon != None
        RegisterForDistanceLessThanEvent(Game.GetPlayer(), siphon, 2500.0)
    EndIf
EndFunction

Function UnregisterSiphonArrival(Int aiSiphonAlias)
    ObjectReference siphon = SiphonRef(aiSiphonAlias)
    If siphon != None
        UnregisterForDistanceEvents(Game.GetPlayer(), siphon)
    EndIf
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If IsStageDone(270) && !IsStageDone(280)
        SetStage(280)
    ElseIf IsStageDone(315) && !IsStageDone(320)
        SetStage(320)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Int phase = ActivePhase()
    If phase == 0
        Return
    EndIf
    If aiTimerID == 1
        ObjectReference siphon = PhaseSiphon(phase)
        If ProbeScript != None && siphon != None && !siphon.IsDestroyed()
            ProbeScript.Pulse(siphon)
        EndIf
        StartTimer(TimeBetweenExtractorPulses, 1)
    ElseIf aiTimerID == 2
        If phase == 3
            LaunchMissileStrike(NumMissilesPerStrike_FinalWave)
        Else
            LaunchMissileStrike(NumMissilesPerStrike)
        EndIf
        StartTimer(TimeBetweenMissileStrikes, 2)
    ElseIf aiTimerID == 3
        WatchWaveActors(phase)
        StartTimer(2.0, 3)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    CancelTimer(2)
    CancelTimer(3)
    UnregisterSiphonArrival(19)
    UnregisterSiphonArrival(20)
    If EWS != None
        UnregisterForCustomEvent(EWS, "WaveComplete")
        EWS.StopAllEncounterWaves()
    EndIf
    If ProbeScript != None && Alias_Activators_AllProbes != None
        Int index = 0
        While index < Alias_Activators_AllProbes.GetCount()
            ProbeScript.SetShield(Alias_Activators_AllProbes.GetAt(index), False)
            index += 1
        EndWhile
    EndIf
EndEvent

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    Int phase = ActivePhase()
    If phase > 0 && akArgs.Length > 0 && (akArgs[0] as Int) == phase - 1
        EWS.StartEncounterWave(phase - 1)
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    UnregisterForRemoteEvent(akSender, "OnDeath")
    If IsStageDone(200) && !IsStageDone(250)
        CountKill("A", 250)
    ElseIf IsStageDone(280) && !IsStageDone(300)
        CountKill("B", 300)
    ElseIf IsStageDone(320) && !IsStageDone(500)
        CountKill("C", 500)
    EndIf
EndEvent

Function CountKill(String asPhase, Int aiBossStage)
    Int kills = GetKills(asPhase) + 1
    SetKills(asPhase, kills)
    If kills >= KillsRequired()
        SetStage(aiBossStage)
    EndIf
EndFunction

Function WatchWaveActors(Int aiPhase)
    If EWS == None
        Return
    EndIf
    RefCollectionAlias aliens = EWS.EncounterWaves[aiPhase - 1].WaveRefCollection
    If aliens == None
        Return
    EndIf
    Int index = 0
    While index < aliens.GetCount()
        Actor alien = aliens.GetAt(index) as Actor
        If alien != None && !alien.IsDead()
            RegisterForRemoteEvent(alien, "OnDeath")
        EndIf
        index += 1
    EndWhile
EndFunction

Function LaunchMissileStrike(Int aiCount)
    Activator marker = Game.GetFormFromFile(0x62EEE7, "SeventySix.esm") as Activator
    Actor player = Game.GetPlayer()
    If marker == None || player.IsInInterior()
        Return
    EndIf
    Int index = 0
    While index < aiCount
        Float angle = Utility.RandomFloat(0.0, 360.0)
        Float range = Utility.RandomFloat(0.0, MissileStrikeRadius)
        ObjectReference strike = player.PlaceAtMe(marker)
        If strike != None
            strike.SetPosition(player.GetPositionX() + range * Math.Cos(angle), player.GetPositionY() + range * Math.Sin(angle), player.GetPositionZ())
        EndIf
        index += 1
    EndWhile
EndFunction

Int Function ActivePhase()
    If IsStageDone(320) && !IsStageDone(9000)
        Return 3
    ElseIf IsStageDone(280) && !IsStageDone(315)
        Return 2
    ElseIf IsStageDone(200) && !IsStageDone(270)
        Return 1
    EndIf
    Return 0
EndFunction

ObjectReference Function PhaseSiphon(Int aiPhase)
    If aiPhase == 1
        Return SiphonRef(4)
    ElseIf aiPhase == 2
        Return SiphonRef(19)
    EndIf
    Return SiphonRef(20)
EndFunction

ObjectReference Function SiphonRef(Int aiAliasID)
    ReferenceAlias siphon = GetAlias(aiAliasID) as ReferenceAlias
    If siphon != None
        Return siphon.GetReference()
    EndIf
    Return None
EndFunction

Int Function KillsRequired()
    DefaultCounterQuestA counter = (Self as Quest) as DefaultCounterQuestA
    If counter != None && counter.TargetValue > 0
        Return counter.TargetValue
    EndIf
    Return 30
EndFunction

Int Function GetKills(String asPhase)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        Return questVariables.GetVariable("AliensKilled" + asPhase) as Int
    EndIf
    Return 0
EndFunction

Function SetKills(String asPhase, Int aiKills)
    SetDisplayVariable("AliensKilled" + asPhase, aiKills)
EndFunction

Function SetDisplayVariable(String asName, Float afValue)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(asName, afValue)
    EndIf
EndFunction
