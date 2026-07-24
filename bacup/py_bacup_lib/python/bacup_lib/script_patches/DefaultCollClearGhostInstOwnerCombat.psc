Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, int aeCombatState)
    If !DoOnce && aeCombatState == 1 && InstanceOwner && akTarget == InstanceOwner.GetActorReference()
        Int i = 0
        Int count = (Self as RefCollectionAlias).GetCount()
        While i < count
            ObjectReference collectionRef = (Self as RefCollectionAlias).GetAt(i)
            Actor collectionActor = collectionRef as Actor
            If collectionActor
                collectionActor.SetGhost(False)
            EndIf
            i += 1
        EndWhile
        DoOnce = True
    EndIf
EndEvent
