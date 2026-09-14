Actor Function EN05MQ_GetPlayer()
    Actor player = None
    If Alias_currentPlayer != None
        player = Alias_currentPlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

Function Fragment_Stage_0005_Item_00()
    SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0007_Item_00()
    Actor player = EN05MQ_GetPlayer()
    EN05_MQ_QuestScript controller = (Self as Quest) as EN05_MQ_QuestScript
    If player != None && controller != None && controller.EN05_MQ_StageValue != None
        player.SetValue(controller.EN05_MQ_StageValue, controller.fDirectToRegisterValue)
    EndIf
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(7)
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor player = EN05MQ_GetPlayer()
    EN05_MQ_QuestScript controller = (Self as Quest) as EN05_MQ_QuestScript
    If player != None && controller != None && controller.EN05_MQ_StageValue != None
        player.SetValue(controller.EN05_MQ_StageValue, controller.fSystemActiveValue)
    EndIf
    SetObjectiveCompleted(7)
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(15)
    SetObjectiveDisplayed(20)
    SetObjectiveDisplayed(25)
    If controller != None
        controller.EN05MQ_ReconcileCommendations(player)
    EndIf
EndFunction

Function Fragment_Stage_0025_Item_00()
    Actor player = EN05MQ_GetPlayer()
    If player != None && EN05_Officer_StartedPresidentalRaceValue != None
        player.SetValue(EN05_Officer_StartedPresidentalRaceValue, 1.0)
    EndIf
    SetObjectiveCompleted(25)
    If !IsStageDone(105)
        SetStage(105)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveSkipped(15)
    SetObjectiveSkipped(20)
    SetObjectiveSkipped(25)
    SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetObjectiveCompleted(10)
    If !IsStageDone(110)
        SetStage(110)
    EndIf
EndFunction

Function Fragment_Stage_0107_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    CompleteAllObjectives()

    Actor player = EN05MQ_GetPlayer()
    If player != None
        If EN05_MQ_CompletedValue != None
            player.SetValue(EN05_MQ_CompletedValue, 1.0)
        EndIf
        If EN05_Officer_HelpedMODUSOnceValue != None
            player.SetValue(EN05_Officer_HelpedMODUSOnceValue, 1.0)
        EndIf
    EndIf

    If EN07_IntroMiscQuestStartKeyword != None
        EN07_IntroMiscQuestStartKeyword.SendStoryEvent(None, player, None, 0, 0)
    EndIf
EndFunction
