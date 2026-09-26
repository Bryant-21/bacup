; This is the first-meeting variant (Storm_MQ01_MetAlyssa == 0 on the player);
; its three siblings need the value at 1. The converted VMAD leaves AlyssaAV
; unbound, so fall back to the record the conditions test.
Function Fragment_End(ObjectReference akSpeakerRef)
    ActorValue metAlyssa = AlyssaAV
    If metAlyssa == None
        metAlyssa = Game.GetFormFromFile(0x00783A70, "SeventySix.esm") as ActorValue
    EndIf
    Actor playerRef = Game.GetPlayer()
    If metAlyssa != None && playerRef != None && playerRef.GetValue(metAlyssa) < 1.0
        playerRef.SetValue(metAlyssa, 1.0)
    EndIf
EndFunction
