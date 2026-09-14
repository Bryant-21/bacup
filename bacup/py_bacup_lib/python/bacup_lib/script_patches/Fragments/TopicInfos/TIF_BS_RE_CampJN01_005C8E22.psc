Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None && BiometricScanner != None
        Game.GetPlayer().RemoveItem(BiometricScanner, 1, True, akSpeakerRef)
    EndIf
EndFunction
