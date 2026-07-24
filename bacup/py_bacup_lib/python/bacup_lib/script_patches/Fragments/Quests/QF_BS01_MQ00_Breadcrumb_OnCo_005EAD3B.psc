Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS01_MQ00_Breadcrumb != None && BS01_MQ00_Breadcrumb_QuestStartKeyword != None
        If !BS01_MQ00_Breadcrumb.IsRunning() && !BS01_MQ00_Breadcrumb.IsCompleted()
            BS01_MQ00_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
