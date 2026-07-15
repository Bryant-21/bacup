Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef
        Return
    EndIf

    If playerRef.GetValue(W05_MQ_002P_Radical_PlayerHeardRoperIntro) > 0.0
        Return
    EndIf

    playerRef.SetValue(W05_MQ_002P_Radical_PlayerHeardRoperIntro, 1.0)
    playerRef.SetValue(W05_MQ_002P_Radical_RoperJackyEnabled, 1.0)
    If W05_DialogueRadicals_JackyRoperIntro != None && !W05_DialogueRadicals_JackyRoperIntro.IsPlaying()
        W05_DialogueRadicals_JackyRoperIntro.Start()
    EndIf
EndEvent
