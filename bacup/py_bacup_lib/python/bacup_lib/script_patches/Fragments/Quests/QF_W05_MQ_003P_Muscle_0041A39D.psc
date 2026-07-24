; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If W05_MQ_003P_Muscle_0100_StartScene
        W05_MQ_003P_Muscle_0100_StartScene.Start()
    ElseIf !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
    If W05_MQ_003P_Muscle_0400_SolAttactScene
        W05_MQ_003P_Muscle_0400_SolAttactScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveDisplayed(1020)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1150_Item_00()
    SetObjectiveDisplayed(1150)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1205_Item_00()
    SetObjectiveDisplayed(1205)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
EndFunction

Function Fragment_Stage_1390_Item_00()
    SetObjectiveDisplayed(1390)
EndFunction

Function Fragment_Stage_0499_Item_00()
    If W05_MQ_003P_Muscle_0500_SolExitsScene
        W05_MQ_003P_Muscle_0500_SolExitsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference gauleyMarker = Alias_Sol_GauleyMine_EnableMarker.GetReference()
    ObjectReference waywardMarker = Alias_Sol_Wayward_EnableMarker.GetReference()
    If gauleyMarker
        gauleyMarker.Disable()
    EndIf
    If waywardMarker
        waywardMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1005_Item_00()
    If W05_MQ_003P_Muscle_1005_ReturnScene
        W05_MQ_003P_Muscle_1005_ReturnScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1224_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomCard, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1225_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(W05_MQ_003P_Muscle_HandyRoomKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1230_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerKilledSkinner, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If W05_MQ_003P_Muscle_1310_ReturnScene
        W05_MQ_003P_Muscle_1310_ReturnScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1311_Item_00()
    If W05_MQ_003P_Muscle_1311_RadioScene
        W05_MQ_003P_Muscle_1311_RadioScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1312_Item_00()
    If W05_MQ_003P_Muscle_1311_RadioScene
        W05_MQ_003P_Muscle_1311_RadioScene.Stop()
    EndIf
EndFunction

Function Fragment_Stage_1320_Item_00()
    If W05_MQ_003P_Muscle_1310_ReturnScene
        W05_MQ_003P_Muscle_1310_ReturnScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1320_PlayerReturnsScene
        W05_MQ_003P_Muscle_1320_PlayerReturnsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
    If W05_MQ_004P_Crane_QuestStartKeyword
        W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction
