Event OnQuestInit()
    PlayerRef = Alias_Player.GetActorReference()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
        Alias_Player.ForceRefIfEmpty(PlayerRef)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == Stage_CheckForPA
        GHL00_UpdatePowerArmorObjective()
        StartTimer(TimeToCheckPAStatus, 1)
    ElseIf auiStageID >= Stage_StopCheckingForPA
        CancelTimer(1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
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
    SetObjectiveDisplayed(Obj_ExitPA, inPowerArmor)
    SetObjectiveDisplayed(Obj_EnterRadChamber, !inPowerArmor)
EndFunction
