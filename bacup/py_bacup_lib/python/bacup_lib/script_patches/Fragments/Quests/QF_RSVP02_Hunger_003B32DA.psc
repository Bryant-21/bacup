Function Fragment_Stage_0000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If !playerRef
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
        playerRef = Alias_Player.GetActorReference()
    EndIf
    If playerRef
        playerRef.SetValue(pRSVP02_AV_QuestStarted, 1.0)
        playerRef.SetValue(pRSVP00_AV_isVolunteerCandidate, 1.0)
        If playerRef.GetValue(pRSVP00_AV_foundDelbert) > 0.0
            If !IsStageDone(1200)
                SetStage(1200)
            EndIf
        ElseIf !IsStageDone(1000)
            SetStage(1000)
        EndIf
    ElseIf !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 1000.0)
    EndIf
    SetObjectiveDisplayed(1000)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 1200.0)
    EndIf
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1000, False)
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_2000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 2000.0)
        playerRef.SetValue(pRSVP00_AV_foundDelbert, 1.0)
    EndIf
    SetObjectiveCompleted(1000)
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1100, False)
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_2100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 2100.0)
    EndIf
    SetObjectiveCompleted(2000)
    If !IsStageDone(2200)
        SetStage(2200)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 2200.0)
    EndIf
    SetObjectiveDisplayed(2200)
    RegisterRibeyeSubstitute()
    TryPrepareRibeyeSubstitute()
EndFunction

Function Fragment_Stage_3000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 3000.0)
    EndIf
    RemoveAllInventoryEventFilters()
    SetObjectiveCompleted(2200)
    SetObjectiveDisplayed(2210, False)
    SetObjectiveDisplayed(2215, False)
    If !IsStageDone(6000)
        SetStage(6000)
    EndIf
EndFunction

Function Fragment_Stage_6000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 6000.0)
    EndIf
    SetObjectiveDisplayed(6000)
EndFunction

Function Fragment_Stage_7000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Checkpoint, 7000.0)
    EndIf
    SetObjectiveCompleted(6000)
    SetObjectiveDisplayed(7000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    SetObjectiveCompleted(7000)
    If playerRef
        playerRef.SetValue(pRSVP02_AV_Quest_Done, 1.0)
        playerRef.SetValue(pRSVP00_AV_isVolunteer, 1.0)
        playerRef.SetValue(pRSVP00_AV_isVolunteerCandidate, 0.0)
        playerRef.SetValue(pRS01B_Contact_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
    EndIf
    RemoveAllInventoryEventFilters()
    Stop()
EndFunction

Function RegisterRibeyeSubstitute()
    RemoveAllInventoryEventFilters()
    If pBrahminMeat
        AddInventoryEventFilter(pBrahminMeat)
    EndIf
    If pC_Wood
        AddInventoryEventFilter(pC_Wood)
    EndIf
    If pBrahminMeatCooked
        AddInventoryEventFilter(pBrahminMeatCooked)
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && IsStageDone(2200) && !IsStageDone(3000)
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        RegisterForRemoteEvent(playerRef, "OnItemAdded")
    EndIf
EndFunction

Function TryPrepareRibeyeSubstitute()
    Actor playerRef = Alias_Player.GetActorReference()
    If !playerRef || !IsStageDone(2200) || IsStageDone(3000) || !pBrahminMeatCooked
        Return
    EndIf

    If playerRef.GetItemCount(pBrahminMeatCooked) > 0
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        SetStage(3000)
        Return
    EndIf

    If pBrahminMeat && pC_Wood && playerRef.GetItemCount(pBrahminMeat) > 0 && playerRef.GetItemCount(pC_Wood) > 0
        playerRef.RemoveItem(pBrahminMeat, 1, True)
        playerRef.RemoveItem(pC_Wood, 1, True)
        playerRef.AddItem(pBrahminMeatCooked, 1, False)
        If playerRef.GetItemCount(pBrahminMeatCooked) > 0 && !IsStageDone(3000)
            UnregisterForRemoteEvent(playerRef, "OnItemAdded")
            SetStage(3000)
        EndIf
    EndIf
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Actor playerRef = Alias_Player.GetActorReference()
    If akSender == playerRef && aiItemCount > 0 && (akBaseItem == pBrahminMeat || akBaseItem == pC_Wood || akBaseItem == pBrahminMeatCooked)
        TryPrepareRibeyeSubstitute()
    EndIf
EndEvent
