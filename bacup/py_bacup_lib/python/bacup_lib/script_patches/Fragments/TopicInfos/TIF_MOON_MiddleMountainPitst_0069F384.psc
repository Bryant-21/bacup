Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Potion_Beer != None && playerRef.GetItemCount(Potion_Beer) > 0
        playerRef.EquipItem(Potion_Beer, False, False)
        If Message_ENDMid != None
            Message_ENDMid.Show()
        EndIf
    EndIf
EndFunction
