Bool Function IsActivatorBroken()
    String currentState = GetState()
    Return currentState == "open" || currentState == "opening" || currentState == "blockedopen"
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage > 0
        If aiOldStage == 0 && DestructionExplosion != None
            If DestructionExplosionSourceNode != ""
                PlaceAtNode(DestructionExplosionSourceNode, DestructionExplosion)
            Else
                PlaceAtMe(DestructionExplosion)
            EndIf
        EndIf
        SetLocalOpen(True, True)
    ElseIf aiOldStage > 0
        If FixSound != None
            FixSound.Play(Self)
        EndIf
        SetLocalOpen(False, True)
    EndIf
EndEvent

Event OnReset()
    SetLocalOpen(False, False)
EndEvent
