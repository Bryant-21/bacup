Function ApplyActorValuesTo(ObjectReference akRef)
    If akRef == None || ActorValues == None
        Return
    EndIf
    Int index = 0
    While index < ActorValues.Length
        ActorValue valueToSet = ActorValues[index].ActorValueToSet
        If valueToSet != None
            akRef.SetValue(valueToSet, ActorValues[index].ValueToSet)
        EndIf
        index += 1
    EndWhile
EndFunction

Function ApplyActorValuesToCollection()
    Int index = 0
    While index < GetCount()
        ApplyActorValuesTo(GetAt(index))
        index += 1
    EndWhile
EndFunction

Event OnAliasInit()
    ApplyActorValuesToCollection()
EndEvent

Event OnAliasReset()
    ApplyActorValuesToCollection()
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    If UseOnRefAddedTiming
        ApplyActorValuesTo(akSenderRef)
    EndIf
EndEvent
