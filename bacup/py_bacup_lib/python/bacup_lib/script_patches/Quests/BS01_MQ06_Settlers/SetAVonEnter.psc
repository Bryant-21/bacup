Event OnTriggerEnter(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor != Game.GetPlayer() || ActorValuesToSet == None
        Return
    EndIf

    Int valueIndex = 0
    While valueIndex < ActorValuesToSet.Length
        If ActorValuesToSet[valueIndex].ActorValueToSet != None
            enteringActor.SetValue(ActorValuesToSet[valueIndex].ActorValueToSet, ActorValuesToSet[valueIndex].ValueToSet)
        EndIf
        valueIndex += 1
    EndWhile
EndEvent
