Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pBoS02SoldierCertificate != None && playerRef.GetItemCount(pBoS02SoldierCertificate) == 0
        playerRef.AddItem(pBoS02SoldierCertificate, 1, False)
    EndIf
EndEvent
