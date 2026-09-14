Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && playerRef.GetLevel() >= 20 && BS01_MQ00_Breadcrumb != None && BS01_MQ00_Breadcrumb_QuestStartKeyword != None
        If !BS01_MQ00_Breadcrumb.IsRunning() && !BS01_MQ00_Breadcrumb.IsCompleted()
            BS01_MQ00_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
