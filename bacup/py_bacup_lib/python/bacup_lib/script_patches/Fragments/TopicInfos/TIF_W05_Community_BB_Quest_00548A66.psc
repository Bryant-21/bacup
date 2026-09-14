Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If player.GetItemCount(Caps001) >= 5
        player.RemoveItem(Caps001, 5, True)
        CurRenterAlias.ForceRefTo(player)
    EndIf
EndFunction
