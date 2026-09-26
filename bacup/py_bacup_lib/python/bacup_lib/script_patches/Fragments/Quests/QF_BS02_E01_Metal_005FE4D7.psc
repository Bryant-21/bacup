Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(10)
    ResetEventObjective(20)
    ResetEventObjective(30)
    ResetEventObjective(31)
    ResetEventObjective(32)
    ResetEventObjective(33)
    ResetEventObjective(34)
    ResetEventObjective(35)
    ResetEventObjective(36)
    ResetEventObjective(40)
    ResetEventObjective(50)
    ResetEventObjective(55)
    ResetEventObjective(60)
    ResetEventObjective(65)
    ResetEventObjective(70)
    ResetEventObjective(80)
EndFunction

Bool Function IsObjectiveOpen(Int aiObjective)
    Return IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveOpen(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveOpen(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjectives()
    FailOpenObjective(10)
    FailOpenObjective(20)
    FailOpenObjective(30)
    FailOpenObjective(31)
    FailOpenObjective(32)
    FailOpenObjective(33)
    FailOpenObjective(34)
    FailOpenObjective(35)
    FailOpenObjective(36)
    FailOpenObjective(40)
    FailOpenObjective(50)
    FailOpenObjective(55)
    FailOpenObjective(60)
    FailOpenObjective(65)
    FailOpenObjective(70)
    FailOpenObjective(80)
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9992) || IsStageDone(9999)
EndFunction

Quests:BS02_E01_Metal:QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as Quests:BS02_E01_Metal:QuestScript
EndFunction

DefaultQuestEncounterWaveScript Function EventWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None && !IsEventResolved() && !IsStageDone(900)
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWaves()
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function PappasSay(Topic akTopic)
    If akTopic == None || Alias_Actor_InitiatePappas == None
        Return
    EndIf
    Actor pappas = Alias_Actor_InitiatePappas.GetActorReference()
    If pappas != None && !pappas.IsDead()
        pappas.Say(akTopic)
    EndIf
EndFunction

Function StopArenaTimer()
    Quest owner = Self as Quest
    B21:QuestTimer arenaTimer = owner as B21:QuestTimer
    ; Once the arena resolves, a late expiry must not set the FailQuest stage 9991.
    If arenaTimer != None
        arenaTimer.StopQuestTimer()
    EndIf
EndFunction

Function ArmEventFailsafe(Float afDelay)
    Quests:BS02_E01_Metal:QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmFailsafe(afDelay)
    EndIf
EndFunction

Function AddGladiator(ReferenceAlias akMember)
    If akMember == None || Alias_RefCol_BoSGladiators == None
        Return
    EndIf
    ObjectReference member = akMember.GetReference()
    If member != None && Alias_RefCol_BoSGladiators.Find(member) < 0
        Alias_RefCol_BoSGladiators.AddRef(member)
    EndIf
EndFunction

Function SetGladiatorCaptive(ReferenceAlias akMember, Bool abCaptive)
    If akMember == None
        Return
    EndIf
    Actor member = akMember.GetActorReference()
    If member == None || member.IsDead()
        Return
    EndIf
    ; The team ships in the captive faction so nothing attacks it before the bell or after the last round.
    If abCaptive
        If CaptiveFaction != None
            member.AddToFaction(CaptiveFaction)
        EndIf
        member.StopCombat()
    Else
        If CaptiveFaction != None
            member.RemoveFromFaction(CaptiveFaction)
        EndIf
        If BoundCaptiveFaction != None
            member.RemoveFromFaction(BoundCaptiveFaction)
        EndIf
    EndIf
    member.EvaluatePackage()
EndFunction

Function SetTeamCaptive(Bool abCaptive)
    SetGladiatorCaptive(Alias_Actor_BoSRifleman, abCaptive)
    SetGladiatorCaptive(Alias_Actor_BoSScout, abCaptive)
    SetGladiatorCaptive(Alias_Actor_BoSTechnician, abCaptive)
EndFunction

Function LinkHoldPosition(ReferenceAlias akMember, ReferenceAlias akTrigger)
    If akMember == None || akTrigger == None || DMP_Combat_HoldPosition == None
        Return
    EndIf
    Actor member = akMember.GetActorReference()
    ObjectReference holdTrigger = akTrigger.GetReference()
    If member != None && holdTrigger != None
        member.SetLinkedRef(holdTrigger, DMP_Combat_HoldPosition)
        member.EvaluatePackage()
    EndIf
EndFunction

Function RestGladiator(ReferenceAlias akMember)
    If akMember == None || HealthAV == None
        Return
    EndIf
    Actor member = akMember.GetActorReference()
    If member != None && !member.IsDead() && !member.IsBleedingOut()
        member.RestoreValue(HealthAV, 100000.0)
    EndIf
EndFunction

Function RestTeam()
    RestGladiator(Alias_Actor_BoSRifleman)
    RestGladiator(Alias_Actor_BoSScout)
    RestGladiator(Alias_Actor_BoSTechnician)
EndFunction

Bool Function IsGladiatorAlive(ReferenceAlias akMember)
    If akMember == None
        Return False
    EndIf
    Actor member = akMember.GetActorReference()
    Return member != None && !member.IsDead()
EndFunction

Int Function DeadGladiatorCount(Int aiDyingIndex)
    Int dead = 0
    Int member = 0
    While member < 3
        If member == aiDyingIndex || IsStageDone(901 + member)
            dead += 1
        EndIf
        member += 1
    EndWhile
    Return dead
EndFunction

Function GladiatorDied(ReferenceAlias akMember, Int aiMemberIndex)
    FailOpenObjective(31 + aiMemberIndex)
    FailOpenObjective(34 + aiMemberIndex)
    If akMember != None
        Actor member = akMember.GetActorReference()
        If member != None && !member.IsDead()
            ; The heal countdown ran out, so the downed gladiator does not get back up.
            member.KillEssential(None)
        EndIf
    EndIf
    Int dead = DeadGladiatorCount(aiMemberIndex)
    If BS02_E01_Metal_BrotherhoodNPCsDead != None
        BS02_E01_Metal_BrotherhoodNPCsDead.SetValue(dead as Float)
    EndIf
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    If dead >= 3
        SetStage(900)
    Else
        StartEventScene(Scene_BoSKilled)
    EndIf
EndFunction

Function SpawnGoldenEyebot()
    If Alias_Actor_GoldenEyebot2 == None || Marker_GoldenEyebotSpawn == None || ActorBase_GoldenEyebot == None
        Return
    EndIf
    ObjectReference spawnMarker = Marker_GoldenEyebotSpawn.GetReference()
    If spawnMarker == None || Alias_Actor_GoldenEyebot2.GetReference() != None
        Return
    EndIf
    Actor eyebot = spawnMarker.PlaceActorAtMe(ActorBase_GoldenEyebot)
    If eyebot == None
        Return
    EndIf
    Alias_Actor_GoldenEyebot2.ForceRefTo(eyebot)
    If Trigger_GoldenEyebotPatrol != None && Trigger_GoldenEyebotPatrol.GetReference() != None && DMP_Patrol_Run != None
        eyebot.SetLinkedRef(Trigger_GoldenEyebotPatrol.GetReference(), DMP_Patrol_Run)
    EndIf
    eyebot.EvaluatePackage()
    SetObjectiveDisplayed(55, True, True)
    PappasSay(Topic_GoldenEyeBot)
    Quests:BS02_E01_Metal:QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.AnnounceGoldenEyebotBonus()
    EndIf
EndFunction

Function RemoveGoldenEyebot()
    If Alias_Actor_GoldenEyebot2 == None
        Return
    EndIf
    Actor eyebot = Alias_Actor_GoldenEyebot2.GetActorReference()
    ; Clearing the alias first keeps a late kill from paying the bonus after it closed.
    Alias_Actor_GoldenEyebot2.Clear()
    If eyebot != None
        If !eyebot.IsDead()
            eyebot.DisableNoWait(True)
        EndIf
        eyebot.Delete()
    EndIf
EndFunction

Function CloseGoldenEyebotBonus(Bool abForce)
    If IsStageDone(401) || IsStageDone(402) || !IsObjectiveOpen(55)
        Return
    EndIf
    If !abForce && Alias_Actor_GoldenEyebot2 != None && Scene_GoldenEyebotLeaves != None
        Actor eyebot = Alias_Actor_GoldenEyebot2.GetActorReference()
        If eyebot != None && !eyebot.IsDead()
            ; The exit scene sets 402 when the eyebot has flown off; downtime 2 closes it if the scene cannot.
            StartEventScene(Scene_GoldenEyebotLeaves)
            Return
        EndIf
    EndIf
    SetStage(402)
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    If BS02_E01_Metal_BrotherhoodNPCsDead != None
        BS02_E01_Metal_BrotherhoodNPCsDead.SetValue(0.0)
    EndIf
    ; GladiatorBleedoutScript watches this collection, and nothing else fills it.
    AddGladiator(Alias_Actor_BoSRifleman)
    AddGladiator(Alias_Actor_BoSScout)
    AddGladiator(Alias_Actor_BoSTechnician)
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(10)
    If IsEventResolved() || IsStageDone(300)
        Return
    EndIf
    SetObjectiveDisplayed(20, True, True)
    StartEventScene(Scene_ArenaIntro)
EndFunction

Function Fragment_Stage_0300_Item_00()
    If Music_CombatMusic != None
        Music_CombatMusic.Add()
    EndIf
    CompleteOpenObjective(10)
    CompleteOpenObjective(20)
    If IsEventResolved()
        Return
    EndIf
    SetTeamCaptive(False)
    LinkHoldPosition(Alias_Actor_BoSRifleman, Trigger_RiflemanHoldPos)
    LinkHoldPosition(Alias_Actor_BoSTechnician, Trigger_TechnicianHoldPos)
    SetObjectiveDisplayed(30, True, True)
    Int member = 0
    While member < 3
        SetObjectiveDisplayed(31 + member, True, True)
        If IsStageDone(901 + member)
            FailOpenObjective(31 + member)
        EndIf
        member += 1
    EndWhile
    SetObjectiveDisplayed(40, True, True)
    StartEventWave("Round 1")
    StartEventWave("Eyebombs")
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteOpenObjective(40)
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    RestTeam()
    SetObjectiveDisplayed(50, True, True)
    PappasSay(Topic_Round1End)
    Quests:BS02_E01_Metal:QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ScheduleRoundIntro(2)
    EndIf
    SpawnGoldenEyebot()
EndFunction

Function Fragment_Stage_0401_Item_00()
    CompleteOpenObjective(55)
EndFunction

Function Fragment_Stage_0402_Item_00()
    FailOpenObjective(55)
    If !IsStageDone(401)
        RemoveGoldenEyebot()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    CompleteOpenObjective(50)
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    CloseGoldenEyebotBonus(False)
    SetObjectiveDisplayed(60, True, True)
    StartEventWave("Round 2")
EndFunction

Function Fragment_Stage_0600_Item_00()
    CompleteOpenObjective(60)
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    CloseGoldenEyebotBonus(True)
    RestTeam()
    SetObjectiveDisplayed(70, True, True)
    SetObjectiveDisplayed(65, True, True)
    PappasSay(Topic_Round2End)
    Quests:BS02_E01_Metal:QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ScheduleRoundIntro(3)
        eventScript.AnnounceEmoteBonus()
    EndIf
EndFunction

Function Fragment_Stage_0601_Item_00()
    CompleteOpenObjective(65)
EndFunction

Function Fragment_Stage_0700_Item_00()
    CompleteOpenObjective(70)
    FailOpenObjective(65)
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    SetObjectiveDisplayed(80, True, True)
    StartEventWave("Boss Minions")
EndFunction

Function Fragment_Stage_0750_Item_00()
    StartEventWave("Round 3 - Boss")
EndFunction

Function Fragment_Stage_0800_Item_00()
    CompleteOpenObjective(80)
    If IsEventResolved() || IsStageDone(900)
        Return
    EndIf
    StopArenaTimer()
    StopEventWaves()
    SetTeamCaptive(True)
    ; Pappas' closing topic sets 9000 when it ends; the failsafe sets it if he never finishes.
    PappasSay(Topic_Round3End_Win)
    ArmEventFailsafe(20.0)
EndFunction

Function Fragment_Stage_0900_Item_00()
    If IsEventResolved()
        Return
    EndIf
    StopArenaTimer()
    FailOpenObjectives()
    StopEventWaves()
    ; The failure scene's closing line sets 9992; the failsafe sets it if the scene cannot play.
    StartEventScene(Scene_Failure)
    ArmEventFailsafe(20.0)
EndFunction

Function Fragment_Stage_0901_Item_00()
    GladiatorDied(Alias_Actor_BoSRifleman, 0)
EndFunction

Function Fragment_Stage_0902_Item_00()
    GladiatorDied(Alias_Actor_BoSScout, 1)
EndFunction

Function Fragment_Stage_0903_Item_00()
    GladiatorDied(Alias_Actor_BoSTechnician, 2)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsStageDone(9991) || IsStageDone(9992) || IsStageDone(9999)
        Return
    EndIf
    CompleteOpenObjective(80)
    If IsGladiatorAlive(Alias_Actor_BoSRifleman)
        CompleteOpenObjective(31)
    EndIf
    If IsGladiatorAlive(Alias_Actor_BoSScout)
        CompleteOpenObjective(32)
    EndIf
    If IsGladiatorAlive(Alias_Actor_BoSTechnician)
        CompleteOpenObjective(33)
    EndIf
    CompleteOpenObjective(30)
    FailOpenObjectives()
    StopArenaTimer()
    StopEventWaves()
    SetTeamCaptive(True)
    StartEventScene(Scene_Success)
    ; The team's exit scene sets 10000 when it ends; the failsafe stops the event if it cannot play.
    StartEventScene(Scene_BoSTeamExit)
    ArmEventFailsafe(30.0)
EndFunction

Function Fragment_Stage_9991_Item_00()
    If IsStageDone(9000) || IsStageDone(9992)
        Return
    EndIf
    FailOpenObjectives()
    StopEventWaves()
    SetTeamCaptive(True)
    ArmEventFailsafe(15.0)
EndFunction

Function Fragment_Stage_9992_Item_00()
    If IsStageDone(9000) || IsStageDone(9991)
        Return
    EndIf
    StopArenaTimer()
    FailOpenObjectives()
    StopEventWaves()
    SetTeamCaptive(True)
    ArmEventFailsafe(15.0)
EndFunction

Function Fragment_Stage_9999_Item_00()
    FailOpenObjectives()
    StopEventWaves()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    If Music_CombatMusic != None
        Music_CombatMusic.Remove()
    EndIf
    RemoveGoldenEyebot()
    Stop()
EndFunction
