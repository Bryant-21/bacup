Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If !OnEnd
        ApplyActorValues(akSpeakerRef)
    EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If OnEnd
        ApplyActorValues(akSpeakerRef)
    EndIf
EndEvent

Function ApplyActorValues(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If SetOnSpeaker
        ChangeActorValues(akSpeakerRef)
    EndIf
    Bool changePlayer = SetOnTarget
    If !changePlayer && SetOnNearbyPlayers && playerRef != None && akSpeakerRef != None
        Float distanceLimit = DefaultNearbyDistance
        If NearbyDistance != None
            distanceLimit = NearbyDistance.GetValue()
        EndIf
        changePlayer = akSpeakerRef.GetDistance(playerRef) <= distanceLimit
    EndIf
    If changePlayer && (!SetOnSpeaker || akSpeakerRef != playerRef)
        ChangeActorValues(playerRef)
    EndIf
EndFunction

Function ChangeActorValues(ObjectReference akRecipient)
    If akRecipient == None || ValueData == None
        Return
    EndIf
    Int index = 0
    While index < ValueData.Length
        If ValueData[index].TargetValue != None
            If ValueData[index].SetActorValueToNewValue
                akRecipient.SetValue(ValueData[index].TargetValue, ValueData[index].NewValue)
            ElseIf ValueData[index].UseSetValueToModify
                Float previousValue = akRecipient.GetValue(ValueData[index].TargetValue)
                akRecipient.SetValue(ValueData[index].TargetValue, previousValue + ValueData[index].NewValue)
            Else
                akRecipient.ModValue(ValueData[index].TargetValue, ValueData[index].NewValue)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction
