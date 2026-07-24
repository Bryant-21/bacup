; TODO

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    If W05_MQ_001P_Wayward_400_Scene
        W05_MQ_001P_Wayward_400_Scene.Start()
    EndIf
    If !IsStageDone(405)
        SetStage(405)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0550_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerLearnedRadicalsLocation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0598_Item_00()
    If W05_MQ_001P_Wayward_0599_DuchessInterstatial
        W05_MQ_001P_Wayward_0599_DuchessInterstatial.Start()
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Brew_DuchessDram, 1, False)
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerGotFreeDrink, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAskedAboutOverseer, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0660_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_002, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0680_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerAgreedToHearOutDuchess, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0809_Item_00()
    ObjectReference schematicMarker = Alias_SchematicEnableMarker.GetReference()
    If schematicMarker
        schematicMarker.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    ObjectReference gorgeMapMarker = Alias_GorgeMapMarker.GetReference()
    If gorgeMapMarker
        gorgeMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If W05_MQ_001P_Wayward_500_Scene
        W05_MQ_001P_Wayward_500_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0599_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_BatterDied, 1.0)
    EndIf
    If !IsStageDone(598)
        SetStage(598)
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0805_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0807_Item_00()
    If !IsStageDone(809)
        SetStage(809)
    EndIf
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0905_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If W05_MQ_003P_Muscle_QuestStartKeyword
        W05_MQ_003P_Muscle_QuestStartKeyword.SendStoryEvent()
    EndIf
EndFunction
