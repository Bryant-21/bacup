Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference noviceHolotape = Alias_MoM02NoviceHolotape.GetReference()
    If player && noviceHolotape && noviceHolotape.GetContainer() != player
        player.AddItem(noviceHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(21)
    SetObjectiveDisplayed(22)
    SetObjectiveDisplayed(23)
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0021_Item_00()
    SetObjectiveCompleted(21)
    If IsStageDone(22) && IsStageDone(23) && !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function Fragment_Stage_0022_Item_00()
    SetObjectiveCompleted(22)
    If IsStageDone(21) && IsStageDone(23) && !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function Fragment_Stage_0023_Item_00()
    SetObjectiveCompleted(23)
    If IsStageDone(21) && IsStageDone(22) && !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(90)

    Actor player = Alias_ActivePlayer.GetActorReference()
    If player && MoMRank
        player.SetValue(MoMRank, 3.0)
    EndIf

    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 7
        Quest nextQuest = masterScript.MoMQuestList[7].MoMQuest
        If nextQuest != None && !nextQuest.IsRunning() && !nextQuest.IsCompleted()
            Keyword startKeyword = masterScript.MoMQuestList[7].MoMQuestKeyword
            If startKeyword != None
                startKeyword.SendStoryEventAndWait(None, player)
            EndIf
        EndIf
        If nextQuest != None && nextQuest.IsRunning() && !nextQuest.IsStageDone(20)
            nextQuest.SetStage(20)
        EndIf
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction
