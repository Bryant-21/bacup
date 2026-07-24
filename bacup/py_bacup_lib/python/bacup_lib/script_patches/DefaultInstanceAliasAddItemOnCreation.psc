Event OnInit()
    ObjectReference ownerRef = InstanceOwner.GetRef()
    If ownerRef && ownerRef.GetValue(TurnOffAV) < TurnOffAVValue
        ObjectReference newRef = ownerRef.PlaceAtMe(ItemtoAdd)
        If ItemDestinationAlias
            ItemDestinationAlias.ForceRefTo(newRef)
        EndIf
        ownerRef.SetValue(TurnOffAV, TurnOffAVValue)
    EndIf
EndEvent
