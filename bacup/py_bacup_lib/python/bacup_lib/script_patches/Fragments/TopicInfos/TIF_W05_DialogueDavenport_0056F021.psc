Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || W05_LookingForCameraAV == None
        Return
    EndIf
    If playerRef.GetValue(W05_LookingForCameraAV) != 0.0
        Return
    EndIf
    If P01C_Bucket != None && !P01C_Bucket.IsRunning() && !P01C_Bucket.IsCompleted()
        P01C_Bucket.Start()
    EndIf
    playerRef.SetValue(W05_LookingForCameraAV, 1.0)
    If P01C_BucketMisc_StartQuestKeyword != None
        P01C_BucketMisc_StartQuestKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
