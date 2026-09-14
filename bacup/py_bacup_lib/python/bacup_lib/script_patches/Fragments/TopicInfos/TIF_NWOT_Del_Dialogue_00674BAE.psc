Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If player != None && Currency_Caps != None && Potion_Bufftats != None && player.GetItemCount(Currency_Caps) >= 80
        player.RemoveItem(Currency_Caps, 80, True)
        player.AddItem(Potion_Bufftats, 1, False)
        player.SetValue(ChemPurchases, 1.0)
    EndIf
EndFunction
