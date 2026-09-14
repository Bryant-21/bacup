Function Fragment_Stage_0010_Item_00()
    Actor playerRef
    If Alias_SFZ14Player != None
        playerRef = Alias_SFZ14Player.GetActorReference()
    EndIf
    If playerRef != None && SFZ14_Bomb_QuestCompletedValue != None && playerRef.GetValue(SFZ14_Bomb_QuestCompletedValue) > 0.0
        SetStage(50)
    Else
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0075_Item_00()
    If IsObjectiveDisplayed(50)
        SetObjectiveCompleted(50)
    EndIf
    If IsObjectiveDisplayed(60)
        SetObjectiveCompleted(60)
    EndIf
    SetObjectiveDisplayed(75)
    If SFZ14_Bomb_BoomerIntroScene != None
        SFZ14_Bomb_BoomerIntroScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(75)
    SFZ14_Bomb_QuestScript questScript = (Self as Quest) as SFZ14_Bomb_QuestScript
    If questScript != None
        questScript.PrepareBombs()
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100)
    SFZ14_Bomb_QuestScript questScript = (Self as Quest) as SFZ14_Bomb_QuestScript
    If questScript != None
        questScript.ClearBombTracking()
    EndIf
    SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef
    If Alias_SFZ14Player != None
        playerRef = Alias_SFZ14Player.GetActorReference()
    EndIf
    If playerRef != None && SFZ14_Bomb_QuestCompletedValue != None && playerRef.GetValue(SFZ14_Bomb_QuestCompletedValue) <= 0.0
        playerRef.SetValue(SFZ14_Bomb_QuestCompletedValue, 1.0)
    EndIf
EndFunction
