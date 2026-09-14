Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference seekerHolotape = Alias_MoM03SeekerHolotape.GetReference()
    If player && seekerHolotape && seekerHolotape.GetContainer() != player
        player.AddItem(seekerHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference missionHolotape = Alias_MoM03MissionHolotape.GetReference()
    If player && missionHolotape && missionHolotape.GetContainer() != player
        player.AddItem(missionHolotape, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)

    ObjectReference marker = Alias_PleasantValleyMapMarker.GetReference()
    If marker
        marker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0055_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(72)
    SetObjectiveDisplayed(73)
EndFunction

Function Fragment_Stage_0075_Item_00()
    SetObjectiveCompleted(73)
EndFunction

Function Fragment_Stage_0080_Item_00()
    SetObjectiveCompleted(72)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0085_Item_00()
    SetObjectiveCompleted(81)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(80)
EndFunction

Function Fragment_Stage_0091_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference cryptosData = Alias_CryptosDataCore.GetReference()
    If player && cryptosData && cryptosData.GetContainer() != player
        player.AddItem(cryptosData, 1, True)
    EndIf
    ObjectReference oliviaCredentials = Alias_OliviasCredentials.GetReference()
    If player && oliviaCredentials && oliviaCredentials.GetContainer() != player
        player.AddItem(oliviaCredentials, 1, True)
    EndIf
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 8
        Quest nextQuest = masterScript.MoMQuestList[8].MoMQuest
        If nextQuest != None && !nextQuest.IsRunning() && !nextQuest.IsCompleted()
            Keyword startKeyword = masterScript.MoMQuestList[8].MoMQuestKeyword
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
