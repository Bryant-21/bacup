; Rocksy's hand-off, the hunter parleys and the report-back all ran through
; player-dialogue scenes whose fragments did not survive the FO76 strip.
; Activating the actor stands in for those conversations.
Function EnsureDialogueListeners()
    If Alias_Rocksy != None && Alias_Rocksy.GetReference() != None
        RegisterForRemoteEvent(Alias_Rocksy.GetReference(), "OnActivate")
    EndIf
    If Alias_Raider != None && Alias_Raider.GetReference() != None
        RegisterForRemoteEvent(Alias_Raider.GetReference(), "OnActivate")
    EndIf
    If Alias_Hunter != None && Alias_Hunter.GetReference() != None
        RegisterForRemoteEvent(Alias_Hunter.GetReference(), "OnActivate")
    EndIf
    If Alias_Hunter2 != None && Alias_Hunter2.GetReference() != None
        RegisterForRemoteEvent(Alias_Hunter2.GetReference(), "OnActivate")
    EndIf
EndFunction

Event OnQuestInit()
    EnsureDialogueListeners()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    EnsureDialogueListeners()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If !IsRunning() || Alias_Player == None
        Return
    EndIf
    Actor player = Alias_Player.GetActorReference()
    If player == None || akActionRef != player
        Return
    EndIf

    If Alias_Rocksy != None && akSender == Alias_Rocksy.GetReference()
        If IsStageDone(5000)
            If !IsStageDone(5100)
                SetStage(5100)
            EndIf
        ElseIf !IsStageDone(200)
            SetStage(100)
            SetStage(200)
        EndIf
        Return
    EndIf
    If !IsStageDone(200) || IsStageDone(1000)
        Return
    EndIf
    If Alias_Raider != None && akSender == Alias_Raider.GetReference() && !IsStageDone(1300)
        SetStage(1300)
    ElseIf Alias_Hunter != None && akSender == Alias_Hunter.GetReference() && !IsStageDone(2000)
        SetStage(2000)
    ElseIf Alias_Hunter2 != None && akSender == Alias_Hunter2.GetReference() && !IsStageDone(3000)
        SetStage(3000)
    EndIf
EndEvent

Function ResolveHunterObjectives()
    Bool hunter1Resolved = IsStageDone(2000) || IsStageDone(2700)
    Bool hunter2Resolved = IsStageDone(3000) || IsStageDone(3700)
    If hunter1Resolved && hunter2Resolved
        SetObjectiveDisplayed(2000, False)
        SetObjectiveCompleted(2005, True)
        SetObjectiveCompleted(2010, True)
        SetObjectiveCompleted(2000, True)
    ElseIf hunter1Resolved
        SetObjectiveDisplayed(2000, False)
        SetObjectiveCompleted(2005, True)
        SetObjectiveDisplayed(2010, True, True)
    ElseIf hunter2Resolved
        SetObjectiveDisplayed(2000, False)
        SetObjectiveCompleted(2010, True)
        SetObjectiveDisplayed(2005, True, True)
    EndIf
EndFunction

Function CloseOptionalObjectives()
    SetObjectiveDisplayed(2000, False)
    SetObjectiveDisplayed(2005, False)
    SetObjectiveDisplayed(2010, False)
    SetObjectiveDisplayed(2500, False)
    SetObjectiveDisplayed(2505, False)
EndFunction

; Debug stages (DEBUG in the stage notes) and the two "--- ACTION ---"/"--- END ---"
; separator stages are bound by VMAD but carry no shipped behavior.
Function Fragment_Stage_0000_Item_00()
EndFunction

Function Fragment_Stage_0001_Item_00()
EndFunction

Function Fragment_Stage_0005_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
EndFunction

Function Fragment_Stage_0015_Item_00()
EndFunction

Function Fragment_Stage_0020_Item_00()
EndFunction

Function Fragment_Stage_0099_Item_00()
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
    SetObjectiveDisplayed(2000, True, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(1000, True, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetStage(1000)
EndFunction

; Without the parley dialogue the only outcome the records can still express is
; the one Rocksy rewards: the former Raider agrees to come back to Crater.
Function Fragment_Stage_1300_Item_00()
    SetStage(1310)
EndFunction

Function Fragment_Stage_1310_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1320_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1330_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1340_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_1350_Item_00()
EndFunction

Function Fragment_Stage_1360_Item_00()
    SetStage(1000)
EndFunction

Function Fragment_Stage_2000_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2100_Item_00()
EndFunction

Function Fragment_Stage_2200_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2300_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2400_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2500_Item_00()
    SetObjectiveDisplayed(2500, True, True)
EndFunction

Function Fragment_Stage_2510_Item_00()
    SetObjectiveCompleted(2500, True)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2520_Item_00()
    SetObjectiveCompleted(2500, True)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2600_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_2700_Item_00()
    SetObjectiveDisplayed(2500, False)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3000_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3100_Item_00()
EndFunction

Function Fragment_Stage_3200_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3300_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3400_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3500_Item_00()
    SetObjectiveDisplayed(2505, True, True)
EndFunction

Function Fragment_Stage_3510_Item_00()
    SetObjectiveCompleted(2505, True)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3520_Item_00()
    SetObjectiveCompleted(2505, True)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3600_Item_00()
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_3700_Item_00()
    SetObjectiveDisplayed(2505, False)
    ResolveHunterObjectives()
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveCompleted(1000, True)
    SetObjectiveDisplayed(5000, True, True)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveCompleted(5000, True)
    SetStage(9000)
EndFunction

; Caps, armor, scrip and Treasury Notes are paid by the converter-attached
; B21:QuestRewards / B21:CurrencyQuestRewards rows; only reputation is local.
Function Fragment_Stage_9000_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None
        If pReputation_AV_Crater != None && Rep_Mod_DailyR_Add != None
            player.ModValue(pReputation_AV_Crater, Rep_Mod_DailyR_Add.GetValue())
        EndIf
        If pW05_Daily_R02_AV_TimesCompleted != None
            player.ModValue(pW05_Daily_R02_AV_TimesCompleted, 1.0)
        EndIf
    EndIf
    SetObjectiveCompleted(5000, True)
    CloseOptionalObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(1000, False)
    SetObjectiveDisplayed(5000, False)
    CloseOptionalObjectives()
    Stop()
EndFunction

; RunOnStop: the engine sets this while the quest shuts down, so it must not Stop().
Function Fragment_Stage_10000_Item_00()
    UnregisterForAllEvents()
EndFunction
