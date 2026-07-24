Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)
    If OwningPlayer != None && OwningPlayer.GetReference() != None && akActionRef == OwningPlayer.GetReference() && TargetKey != None && OwningPlayer.GetReference().GetItemCount(TargetKey) == 0
        Game.GetPlayer().AddToFaction(EnemyFaction)
    EndIf
EndEvent
