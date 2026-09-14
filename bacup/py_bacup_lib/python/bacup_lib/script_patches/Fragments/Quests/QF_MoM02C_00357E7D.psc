Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(20)
        SetStage(20)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference missionHolotape = Alias_VoiceOfSetHolotape.GetReference()
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
    SetObjectiveDisplayed(40)

    ObjectReference marker = Alias_EMPResearchMapMarker.GetReference()
    If marker
        marker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0052_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(52)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(52)
    SetObjectiveDisplayed(60)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference siphonHolotape = Alias_DataExfiltrationHolotape.GetReference()
    If player && siphonHolotape && siphonHolotape.GetContainer() != player
        player.AddItem(siphonHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0080_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(80)

    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 3
        Quest parentQuest = masterScript.MoMQuestList[3].MoMQuest
        If parentQuest != None && parentQuest.IsRunning() && !parentQuest.IsStageDone(masterScript.CONST_MoM02_CompletedMoM02C)
            parentQuest.SetStage(masterScript.CONST_MoM02_CompletedMoM02C)
        EndIf
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0255_Item_00()
EndFunction
