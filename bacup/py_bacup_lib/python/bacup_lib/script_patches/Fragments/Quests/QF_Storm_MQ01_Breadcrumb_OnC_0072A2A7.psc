Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && Storm_MQ01_Breadcrumb != None && Storm_MQ01_Breadcrumb_QuestStartKeyword != None
        If !Storm_MQ01_Breadcrumb.IsRunning() && !Storm_MQ01_Breadcrumb.IsCompleted()
            Storm_MQ01_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
