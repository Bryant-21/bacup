Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)

    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference playerLogin = Alias_MoM00PlayerLogin.GetReference()
    If player && playerLogin && playerLogin.GetContainer() != player
        player.AddItem(playerLogin, 1, True)
    EndIf
    ObjectReference initiateHolotape = Alias_MoM01InitiateHolotape.GetReference()
    If player && initiateHolotape && initiateHolotape.GetContainer() != player
        player.AddItem(initiateHolotape, 1, True)
    EndIf
    If player && MoMRank
        player.SetValue(MoMRank, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)

    ObjectReference marker = Alias_LewisburgMapMarker.GetReference()
    If marker
        marker.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0057_Item_00()
    If IsStageDone(58) && IsStageDone(59) && !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0058_Item_00()
    If IsStageDone(57) && IsStageDone(59) && !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0059_Item_00()
    If IsStageDone(57) && IsStageDone(58) && !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0062_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    If !IsStageDone(70)
        SetStage(70)
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(70)
    If !IsStageDone(71)
        SetStage(71)
    EndIf
    If !IsStageDone(80)
        SetStage(80)
    EndIf
EndFunction

Function Fragment_Stage_0080_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0085_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(85)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(85)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(90)

    Actor player = Alias_ActivePlayer.GetActorReference()
    If player && MoMRank
        player.SetValue(MoMRank, 2.0)
    EndIf

    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 3
        Quest nextQuest = masterScript.MoMQuestList[3].MoMQuest
        If nextQuest != None && !nextQuest.IsRunning() && !nextQuest.IsCompleted()
            Keyword startKeyword = masterScript.MoMQuestList[3].MoMQuestKeyword
            If startKeyword != None
                startKeyword.SendStoryEventAndWait(None, player)
            EndIf
        EndIf
        If nextQuest != None && nextQuest.IsRunning() && !nextQuest.IsStageDone(10)
            nextQuest.SetStage(10)
        EndIf
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction
