Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(20)
        SetStage(20)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference missionHolotape = Alias_PhantomDeviceHolotape.GetReference()
    If player && missionHolotape && missionHolotape.GetContainer() != player
        player.AddItem(missionHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(41)
    SetObjectiveDisplayed(42)

    ObjectReference gasMarker = Alias_GasCanisterMapMarker.GetReference()
    If gasMarker
        gasMarker.AddToMap()
    EndIf
    ObjectReference stealthMarker = Alias_StealthBoyMapMarker.GetReference()
    If stealthMarker
        stealthMarker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0041_Item_00()
    SetObjectiveCompleted(41)
    If !IsStageDone(43)
        SetStage(43)
    EndIf
    If IsStageDone(42) && !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0042_Item_00()
    SetObjectiveCompleted(42)
    If !IsStageDone(44)
        SetStage(44)
    EndIf
    If IsStageDone(41) && !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0045_Item_00()
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0046_Item_00()
    SetObjectiveCompleted(45)
EndFunction

Function Fragment_Stage_0047_Item_00()
    ObjectReference marker = Alias_StealthBoyMapMarker.GetReference()
    If marker
        marker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(41)
    SetObjectiveCompleted(42)
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(50)

    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 3
        Quest parentQuest = masterScript.MoMQuestList[3].MoMQuest
        If parentQuest != None && parentQuest.IsRunning() && !parentQuest.IsStageDone(masterScript.CONST_MoM02_CompletedMoM02A)
            parentQuest.SetStage(masterScript.CONST_MoM02_CompletedMoM02A)
        EndIf
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0255_Item_00()
EndFunction
