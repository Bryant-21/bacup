Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If player != None && Currency_Caps != None && Potion_XCell != None && player.GetItemCount(Currency_Caps) >= 100
        player.RemoveItem(Currency_Caps, 100, True)
        player.AddItem(Potion_XCell, 1, False)
        player.SetValue(ChemPurchases, 1.0)
    EndIf
EndFunction
