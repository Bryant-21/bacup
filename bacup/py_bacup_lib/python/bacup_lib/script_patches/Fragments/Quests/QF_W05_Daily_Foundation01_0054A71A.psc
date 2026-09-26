; Ward's turn-in ran through player-dialogue scene W05_Daily_F01_Return, whose
; Fragment_End and topic infos did not survive the FO76 strip. Activating Ward
; stands in for that conversation.
ReferenceAlias Function WardAlias()
    Return GetAlias(1) as ReferenceAlias
EndFunction

Function EnsureWardListener()
    ReferenceAlias ward = WardAlias()
    If ward == None
        Return
    EndIf
    ObjectReference wardRef = ward.GetReference()
    If wardRef != None
        RegisterForRemoteEvent(wardRef, "OnActivate")
    EndIf
EndFunction

Event OnQuestInit()
    EnsureWardListener()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    EnsureWardListener()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If !IsRunning() || Alias_Player == None
        Return
    EndIf
    Actor player = Alias_Player.GetActorReference()
    ReferenceAlias ward = WardAlias()
    If player == None || akActionRef != player || ward == None || akSender != ward.GetReference()
        Return
    EndIf
    If IsStageDone(400)
        If !IsStageDone(9990)
            SetStage(9990)
        EndIf
    ElseIf !IsStageDone(200)
        SetStage(200)
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor thief = None
    If Alias_Thief != None
        thief = Alias_Thief.GetActorReference()
    EndIf
    If thief != None && !thief.IsDead()
        SetObjectiveDisplayed(310, True, True)
    Else
        SetObjectiveDisplayed(310, False)
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(310, True)
    SetObjectiveCompleted(300, True)
    SetStage(9998)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_9900_Item_00()
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveDisplayed(310, False)
    SetObjectiveDisplayed(400, False)
    SetStage(9999)
EndFunction

; Caps and scrip for 9995/9998 are paid by the converter-attached
; B21:QuestRewards / B21:CurrencyQuestRewards rows; only reputation is local.
Function Fragment_Stage_9990_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None && Reputation_AV_Foundation != None && Rep_Mod_DailyS_Add != None
        player.ModValue(Reputation_AV_Foundation, Rep_Mod_DailyS_Add.GetValue())
    EndIf
    SetStage(9995)
EndFunction

Function Fragment_Stage_9992_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None && Reputation_AV_Foundation != None && W05_Daily_Foundation01_DonationRepValue != None
        player.ModValue(Reputation_AV_Foundation, W05_Daily_Foundation01_DonationRepValue.GetValue())
    EndIf
    SetStage(9995)
EndFunction

Function Fragment_Stage_9995_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(310, False)
    SetStage(9999)
EndFunction

Function Fragment_Stage_9998_Item_00()
    Actor player = None
    If Alias_Player != None
        player = Alias_Player.GetActorReference()
    EndIf
    If player != None
        If Reputation_AV_Crater != None && Rep_Mod_Add_Small != None
            player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Small.GetValue())
        EndIf
        If Reputation_AV_Foundation != None && Rep_Mod_Subtract_Small != None
            player.ModValue(Reputation_AV_Foundation, Rep_Mod_Subtract_Small.GetValue())
        EndIf
    EndIf
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveDisplayed(310, False)
    SetObjectiveDisplayed(400, False)
    SetStage(9999)
EndFunction

Function Fragment_Stage_9999_Item_00()
    UnregisterForAllEvents()
    Stop()
EndFunction
