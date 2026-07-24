; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0111_Item_00()
    SetObjectiveDisplayed(111)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0399_Item_00()
    SetObjectiveDisplayed(399)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveDisplayed(750)
EndFunction

Function Fragment_Stage_0760_Item_00()
    SetObjectiveDisplayed(760)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    ObjectReference cacheDoor = Alias_CacheDoor.GetReference()
    If cacheDoor
        cacheDoor.Unlock()
        cacheDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
    If W05_MQ_004P_Crane_1200_RoperScene
        W05_MQ_004P_Crane_1200_RoperScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1230_Item_00()
    SetObjectiveDisplayed(1230)
EndFunction

Function Fragment_Stage_0103_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && playerRef.GetValue(W05_MQ_004P_PlayerStartedQuestOnce) < 1.0 && W05_MQ_004P_Crane_0100a_StartScene
        W05_MQ_004P_Crane_0100a_StartScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_004P_PlayerStartedQuestOnce, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If W05_MQ_004P_Crane_0400_MomentOfSilenceScene
        W05_MQ_004P_Crane_0400_MomentOfSilenceScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If IsStageDone(650) && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    If IsStageDone(600) && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_1170_Item_00()
    If FlatwoodsMapMarker
        FlatwoodsMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_1180_Item_00()
    If MorgantownAirportMapMarker
        MorgantownAirportMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_1245_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.RemoveItem(Caps001, 100, False)
    EndIf
EndFunction

Function Fragment_Stage_1265_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Headwear_Radicals, 1, False)
        playerRef.SetValue(W05_MQ_004P_Crane_PlayerReceivedRadicalsGear, 1.0)
    EndIf
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    If W05_MQ_004P_Crane_1300_DuchessAttractScene
        W05_MQ_004P_Crane_1300_DuchessAttractScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    If !Alias_Roper.GetReference() && !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1235_Item_00()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1260_Item_00()
    If !IsStageDone(1261)
        SetStage(1261)
    EndIf
    If IsStageDone(1261) && !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_8999_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
    If W05_MQ_101P_QuestStartKeyword
        W05_MQ_101P_QuestStartKeyword.SendStoryEvent()
    EndIf
EndFunction
