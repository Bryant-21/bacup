Function Fragment_End(ObjectReference akSpeakerRef)
    Actor kVendor = akSpeakerRef as Actor
    If kVendor == None
        Return
    EndIf

    Utility.Wait(0.25)
    kVendor.ShowBarterMenu()
EndFunction
