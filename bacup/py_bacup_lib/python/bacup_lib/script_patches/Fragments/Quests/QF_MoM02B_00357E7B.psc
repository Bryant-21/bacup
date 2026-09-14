Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(20)
        SetStage(20)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference missionHolotape = Alias_BladeOfBastetHolotape.GetReference()
    If player && missionHolotape && missionHolotape.GetContainer() != player
        player.AddItem(missionHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0032_Item_00()
    SetObjectiveDisplayed(31)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(31)
    SetObjectiveDisplayed(40)

    ObjectReference marker = Alias_HistoricSwordMapMarker.GetReference()
    If marker
        marker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0045_Item_00()
    SetObjectiveCompleted(31)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference swingAnalyzer = Alias_SwingAnalyzer.GetReference()
    If player && swingAnalyzer && swingAnalyzer.GetContainer() != player
        player.AddItem(swingAnalyzer, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0048_Item_00()
    SetObjectiveDisplayed(48)
EndFunction

Function Fragment_Stage_0049_Item_00()
    SetObjectiveCompleted(48)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveCompleted(48)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(60)
    If !IsStageDone(70)
        SetStage(70)
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(70)
    Int objectiveIndex = 80
    While objectiveIndex <= 89
        SetObjectiveCompleted(objectiveIndex)
        objectiveIndex += 1
    EndWhile
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(90)

    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 3
        Quest parentQuest = masterScript.MoMQuestList[3].MoMQuest
        If parentQuest != None && parentQuest.IsRunning() && !parentQuest.IsStageDone(masterScript.CONST_MoM02_CompletedMoM02B)
            parentQuest.SetStage(masterScript.CONST_MoM02_CompletedMoM02B)
        EndIf
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0255_Item_00()
EndFunction
