Event OnQuestInit()
    PlayerRef = Alias_Player.GetActorReference()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
        Alias_Player.ForceRefIfEmpty(PlayerRef)
    EndIf
    RegisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")
    If GetStage() == Stage_StopCheckingForPA
        StartTimer(0.25, 2)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == Stage_CheckForPA
        GHL00_UpdatePowerArmorObjective()
        StartTimer(TimeToCheckPAStatus, 1)
    ElseIf auiStageID >= Stage_StopCheckingForPA
        CancelTimer(1)
        If auiStageID == Stage_StopCheckingForPA
            StartTimer(0.25, 2)
        Else
            CancelTimer(2)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 2
        GHL00_CheckTransformation()
        Return
    EndIf
    If aiTimerID != 1 || GetStage() < Stage_CheckForPA || GetStage() >= Stage_StopCheckingForPA
        Return
    EndIf

    GHL00_UpdatePowerArmorObjective()
    StartTimer(TimeToCheckPAStatus, 1)
EndEvent

Function GHL00_UpdatePowerArmorObjective()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf

    Bool inPowerArmor = PlayerRef.IsInPowerArmor()
    If inPowerArmor
        Alias_Player_NoPA.Clear()
    Else
        Alias_Player_NoPA.ForceRefIfEmpty(PlayerRef)
    EndIf
    SetObjectiveDisplayed(Obj_ExitPA, inPowerArmor)
    SetObjectiveDisplayed(Obj_EnterRadChamber, !inPowerArmor)
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
    PlayerRef = akSender
    If GetStage() == Stage_StopCheckingForPA
        StartTimer(0.25, 2)
    ElseIf GetStage() >= Stage_CheckForPA && GetStage() < Stage_StopCheckingForPA
        StartTimer(TimeToCheckPAStatus, 1)
    EndIf
EndEvent

Function GHL00_CheckTransformation()
    If GetStage() != Stage_StopCheckingForPA || GetStageDone(9000)
        Return
    EndIf
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf
    If PlayerRef.IsDead() || PlayerRef.IsInPowerArmor()
        StartTimer(0.25, 2)
        Return
    EndIf
    If PlayerRef.GetValuePercentage(Game.GetHealthAV()) > BlackOutHealthPercentThreshold
        StartTimer(0.25, 2)
        Return
    EndIf
    ; The online character service is replaced by Tales' stage-driven controller.
    ; Mark completion before latent presentation so repeated timer events cannot re-enter.
    SetStage(9000)
    If BlackOutSpell != None
        BlackOutSpell.Cast(PlayerRef, PlayerRef)
    EndIf
    If TeleportMarker != None
        PlayerRef.MoveTo(TeleportMarker)
    EndIf
    If GHL00_Quest_TransformPlayer_MaddoxAwakeningComment != None
        GHL00_Quest_TransformPlayer_MaddoxAwakeningComment.Start()
    EndIf
EndFunction
