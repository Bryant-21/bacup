Event OnEffectStart(Actor akTarget, Actor akCaster)
    Int i = 0
    While i < ActorValuesToSet.Length
        If ActorValuesToSet[i]
            akTarget.SetValue(ActorValuesToSet[i], 1.0)
        EndIf
        i += 1
    EndWhile
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If ResetValuesOnEnd
        Int i = 0
        While i < ActorValuesToSet.Length
            If ActorValuesToSet[i]
                akTarget.SetValue(ActorValuesToSet[i], 0.0)
            EndIf
            i += 1
        EndWhile
    EndIf
EndEvent
