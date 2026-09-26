Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || ReputationAV == None || RepChange == None
        Return
    EndIf
    Float reputation = playerRef.GetValue(ReputationAV) + RepChange.GetValue()
    If reputation < -3000.0
        reputation = -3000.0
    ElseIf reputation > 13000.0
        reputation = 13000.0
    EndIf
    playerRef.SetValue(ReputationAV, reputation)
EndEvent
