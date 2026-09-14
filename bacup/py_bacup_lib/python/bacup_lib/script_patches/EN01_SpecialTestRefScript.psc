Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = akActionRef as Actor
    If playerRef != Game.GetPlayer()
        Return
    EndIf
    If SPECIALTestInitialMessage.Show() != iMessageButtonIndex
        Return
    EndIf
    If playerRef.GetValue(SpecialActorValue) >= iSpecialRank as Float
        playerRef.SetValue(BlockingActorValue, 0.0)
        SPECIALSuccessMessage.Show()
        If EN01_MQ_Bunker_Master != None && !EN01_MQ_Bunker_Master.IsRunning()
            EN01_MQ_Bunker_Master.Start()
        EndIf
    Else
        playerRef.SetValue(BlockingActorValue, 1.0)
        SPECIALFailureMessage.Show()
    EndIf
EndEvent
