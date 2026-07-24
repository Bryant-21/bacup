; TODO

Function Fragment_Stage_0010_Item_00()
    If MTNS01_Intro && MTNS01_Intro.IsStageDone(600)
        SetStage(100)
    Else
        SetStage(50)
        ObjectReference playerRef = Alias_currentPlayer.GetReference()
        MTNS01_Intro_Quest_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0100_Item_01()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveDisplayed(350)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveDisplayed(650)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveDisplayed(950)
EndFunction

Function Fragment_Stage_0970_Item_00()
    SetObjectiveDisplayed(970)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    If !IsStageDone(1050)
        SetStage(1050)
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    If !IsStageDone(1110)
        SetStage(1110)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveDisplayed(1500)
EndFunction

Function Fragment_Stage_0910_Item_00()
    Alias_currentPlayer.GetReference().RemoveItem(W05_MQ_101P_A_DavidHolotapeMeeting, 1, True)
EndFunction

Function Fragment_Stage_0930_Item_00()
    Alias_currentPlayer.GetReference().AddItem(W05_MQ_101P_A_HookUp, 1, True)
EndFunction

Function Fragment_Stage_1110_Item_00()
    Actor megRef = Alias_Meg.GetActorReference()
    If megRef
        megRef.Enable()
        megRef.EvaluatePackage()
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1210_Item_00()
    Alias_currentPlayer.GetReference().RemoveItem(W05_MQ_101P_A_DavidTrophy, 1, True)
EndFunction

Function Fragment_Stage_1420_Item_00()
    SetStage(1500)
EndFunction

Function Fragment_Stage_1450_Item_00()
    SetStage(1500)
EndFunction

Function Fragment_Stage_1530_Item_00()
    Alias_currentPlayer.GetReference().SetValue(W05_PlayerKnows_AppalachiaHasATreasure, 1.0)
EndFunction

Function Fragment_Stage_8000_Item_00()
    Alias_currentPlayer.GetReference().SetValue(W05_MQ_101P_A_AldridgeWatchstationValue, 1.0)
EndFunction

Function Fragment_Stage_9000_Item_00()
    W05_MQ_101P.SetStage(200)
EndFunction
