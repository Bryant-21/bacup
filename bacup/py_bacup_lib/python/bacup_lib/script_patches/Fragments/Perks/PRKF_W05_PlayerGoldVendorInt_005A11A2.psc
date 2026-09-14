Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)
    If akActor == Game.GetPlayer()
        VendorInteractChoiceScript vendorChoice = (Self as Form) as VendorInteractChoiceScript
        If vendorChoice
            vendorChoice.TriggerVendorInteraction(akActor, akTargetRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Entry_02(ObjectReference akTargetRef, Actor akActor)
    If akActor == Game.GetPlayer()
        VendorInteractChoiceScript vendorChoice = (Self as Form) as VendorInteractChoiceScript
        If vendorChoice
            vendorChoice.TriggerVendorInteraction(akActor, akTargetRef)
        EndIf
    EndIf
EndFunction
