Event OnQuestInit()
    TrialPhase = 0
    IsPlayerSitting = False
    Actor playerRef = Game.GetPlayer()
    RegisterForRemoteEvent(playerRef, "OnSit")
    RegisterForRemoteEvent(playerRef, "OnGetUp")
    RegisterForRemoteEvent(playerRef, "OnDeath")
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    UnregisterForRemoteEvent(playerRef, "OnSit")
    UnregisterForRemoteEvent(playerRef, "OnGetUp")
    UnregisterForRemoteEvent(playerRef, "OnDeath")
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == StartingQuestStage
        BeginTrial()
    ElseIf auiStageID == TrialCompletedStage
        IsPlayerSitting = False
        SetObjectiveCompleted(CompleteTrialObjective)
        CB01_Interrogation_Outro.Start()
    ElseIf auiStageID == PlayerDiedStage
        IsPlayerSitting = False
        ; The QUST VMAD leaves CB01_Interrogation_PlayerDied unbound.
        If CB01_Interrogation_PlayerDied != None
            CB01_Interrogation_PlayerDied.Start()
        EndIf
    EndIf
EndEvent

Function BeginTrial()
    If IsObjectiveDisplayed(SitDownObjective)
        Return
    EndIf
    Actor trialRobot = Robot.GetActorReference()
    If trialRobot != None
        trialRobot.AddPerk(CB01_RobotDamagePerk)
    EndIf
    SetObjectiveDisplayed(SitDownObjective)
    CB01_Interrogation_Intro.Start()
EndFunction

; Chair is alias 4; the scenes only advance while IsPlayerSitting is true.
Event Actor.OnSit(Actor akSender, ObjectReference akFurniture)
    ReferenceAlias chairAlias = GetAlias(4) as ReferenceAlias
    If !IsStageDone(StartingQuestStage) || chairAlias == None || akFurniture == None || akFurniture != chairAlias.GetReference()
        Return
    EndIf
    PlayerInChair.ForceRefTo(akSender)
    IsPlayerSitting = True
    SetObjectiveCompleted(SitDownObjective)
    SetObjectiveDisplayed(CompleteTrialObjective)
EndEvent

Event Actor.OnGetUp(Actor akSender, ObjectReference akFurniture)
    ReferenceAlias chairAlias = GetAlias(4) as ReferenceAlias
    If chairAlias != None && akFurniture == chairAlias.GetReference()
        IsPlayerSitting = False
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If IsStageDone(StartingQuestStage) && !IsStageDone(TrialCompletedStage) && !IsStageDone(PlayerDiedStage)
        SetStage(PlayerDiedStage)
    EndIf
EndEvent

; The intro scene's last phase calls this once to open the first torture round, and each torture round calls it again when it ends.
Function AdvanceTrialPhase()
    If !IsPlayerSitting || IsStageDone(TrialCompletedStage) || IsStageDone(PlayerDiedStage)
        Return
    EndIf
    TrialPhase += 1
    If TrialPhase > NumberOfTrialPhases
        SetStage(TrialCompletedStage)
    ElseIf !CB01_Interrogation_Torture.IsPlaying()
        CB01_Interrogation_Torture.Start()
    EndIf
EndFunction
