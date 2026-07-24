Function Fragment_End(ObjectReference akSpeakerRef)
    If CapsRef != None
        Game.GetPlayer().RemoveItem(CapsRef, 25, true, akSpeakerRef)
    EndIf
    If BrahminMeatRef != None
        Game.GetPlayer().AddItem(BrahminMeatRef, 1)
    EndIf
EndFunction
