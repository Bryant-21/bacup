Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Confirmation = akActionRef == playerRef && playerRef.GetItemCount(EN06_PresidentialSeal) > 0
EndEvent
