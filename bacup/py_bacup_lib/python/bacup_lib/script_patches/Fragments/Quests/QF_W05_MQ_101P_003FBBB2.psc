; TODO

Function Fragment_Stage_0005_Item_00()
    If !IsStageDone(10) && !IsStageDone(20)
        SetStage(10)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_101P_Radio_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_0013_Item_00()
    SetObjectiveDisplayed(13)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveDisplayed(30)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    RS03_Inoculation_Keyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_101P_A_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    W05_MQ_101P_B_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
    SetStage(350)
    If IsStageDone(300)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetStage(351)
    If IsStageDone(200)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If W05_MQ_101P_003_ColaPlantEntranceScene && !W05_MQ_101P_003_ColaPlantEntranceScene.IsPlaying()
        W05_MQ_101P_003_ColaPlantEntranceScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If IsStageDone(1400)
        SetStage(1450)
    Else
        SetStage(1410)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    If IsStageDone(1000)
        SetStage(1450)
    Else
        SetStage(1420)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    If W05_MQ_101P_005b_OverseerHelps && !W05_MQ_101P_005b_OverseerHelps.IsPlaying()
        W05_MQ_101P_005b_OverseerHelps.Start()
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetStage(1610)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetStage(1750)
EndFunction

Function Fragment_Stage_1800_Item_00()
    If IsStageDone(1900)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    If IsStageDone(1800)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_102P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction
