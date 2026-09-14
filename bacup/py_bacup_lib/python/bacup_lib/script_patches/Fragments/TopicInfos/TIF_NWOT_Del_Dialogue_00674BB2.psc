Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If player != None && Currency_Caps != None && Potion_Fury != None && player.GetItemCount(Currency_Caps) >= 50
        player.RemoveItem(Currency_Caps, 50, True)
        player.AddItem(Potion_Fury, 1, False)
        player.SetValue(ChemPurchases, 1.0)
    EndIf
EndFunction
