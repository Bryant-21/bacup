; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveDisplayed(125)
EndFunction

Function Fragment_Stage_0130_Item_00()
    SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0475_Item_00()
    SetObjectiveDisplayed(475)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0998_Item_00()
    SetObjectiveDisplayed(998)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveDisplayed(1020)
EndFunction

Function Fragment_Stage_1030_Item_00()
    SetObjectiveDisplayed(1030)
EndFunction

Function Fragment_Stage_1210_Item_00()
    SetObjectiveDisplayed(1210)
EndFunction

Function Fragment_Stage_1310_Item_00()
    SetObjectiveDisplayed(1310)
EndFunction

Function Fragment_Stage_1320_Item_00()
    SetObjectiveDisplayed(1320)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveDisplayed(1600)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_0140_Item_00()
    SetObjectiveCompleted(125)
EndFunction

Function Fragment_Stage_0505_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerWasAJerkToFirstEnc, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_FirstEncDismissed, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    If W05_MQ_002P_Radical_600_GangerScene
        W05_MQ_002P_Radical_600_GangerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0709_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsRadicalsLocation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_SecondEncDismissedFast, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0736_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1315_Item_00()
    If !IsStageDone(1320)
        SetStage(1320)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_SplitTreasureWithRoper, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    If !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_2115_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerConnectedRadioStation, 1.0)
    EndIf
    If !IsStageDone(709)
        SetStage(709)
    EndIf
    If !IsStageDone(736)
        SetStage(736)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1550_Item_00()
    If !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_8950_Item_00()
    If W05_MQ_003P_Muscle_QuestStartKeyword
        W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction
