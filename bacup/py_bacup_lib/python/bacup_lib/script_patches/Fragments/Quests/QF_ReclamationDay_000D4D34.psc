Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0012_Item_00()
    If IsObjectiveDisplayed(15)
        SetObjectiveCompleted(15)
    EndIf
    If IsObjectiveDisplayed(20)
        SetObjectiveCompleted(20)
    EndIf
    If !IsObjectiveCompleted(10)
        SetObjectiveDisplayed(10)
    EndIf
EndFunction

Function Fragment_Stage_0013_Item_00()
    If !IsObjectiveCompleted(15)
        SetObjectiveDisplayed(15)
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    If AldertonScene && !AldertonScene.IsPlaying()
        AldertonScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(15)
    If !IsStageDone(12)
        SetObjectiveDisplayed(20)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If W05_MQ_001P_Wayward_QuestStartKeyword
        ObjectReference playerRef = Game.GetPlayer()
        W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If !IsStageDone(999)
        SetStage(999)
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    If IsObjectiveDisplayed(15)
        SetObjectiveCompleted(15)
    EndIf
    If IsObjectiveDisplayed(20)
        SetObjectiveCompleted(20)
    EndIf
    SetObjectiveCompleted(10)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If RS01A_Contact_Keyword
            RS01A_Contact_Keyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
        If W05_MQ_101P_QuestStartKeyword
            W05_MQ_101P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
        If BS01_MQ00_Breadcrumb_QuestStartKeyword
            BS01_MQ00_Breadcrumb_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
    EndIf
    If NPE_HelpMenu_Message
        NPE_HelpMenu_Message.Show()
    EndIf
EndFunction
