Function DeployRecalibratedLiberator()
    Actor deployPlayer = Alias_Player.GetActorReference()
    If deployPlayer == None
        deployPlayer = Game.GetPlayer()
    EndIf
    If deployPlayer == None
        Return
    EndIf

    ObjectReference liberatorRef = Alias_Actor_RecalibratedLiberator.GetReference()
    If liberatorRef == None
        liberatorRef = deployPlayer.PlaceAtMe(W05_MQS_202P_RecalibratedLiberator, 1, False, False)
        If liberatorRef != None
            Alias_Actor_RecalibratedLiberator.ForceRefTo(liberatorRef)
        EndIf
    Else
        liberatorRef.MoveTo(deployPlayer)
        liberatorRef.Enable()
    EndIf

    Actor liberatorActor = liberatorRef as Actor
    If liberatorActor != None
        liberatorActor.EvaluatePackage()
    EndIf
EndFunction
