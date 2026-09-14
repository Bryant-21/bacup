Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || W05_LookingForCameraAV == None || P01C_Bucket == None
        Return
    EndIf
    If playerRef.GetValue(W05_LookingForCameraAV) != 0.0
        Return
    EndIf

    Bool started = P01C_Bucket.IsRunning() || P01C_Bucket.IsCompleted()
    If !started
        Keyword bucketStartKeyword = Game.GetFormFromFile(0x004845B4, "SeventySix.esm") as Keyword
        If bucketStartKeyword != None
            started = bucketStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    If !started || (!P01C_Bucket.IsRunning() && !P01C_Bucket.IsCompleted())
        Return
    EndIf

    playerRef.SetValue(W05_LookingForCameraAV, 1.0)
    If P01C_Bucket.IsRunning() && P01C_BucketMisc_StartQuestKeyword != None
        P01C_BucketMisc_StartQuestKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
