Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || !SFL02_Track.IsRunning() || !playerRef.HasKeyword(SFL02_Track_QuestActiveKeyword)
		Return
	EndIf
	SFL02VertibotPlayers.AddRef(playerRef)
	playerRef.AddPerk(SFL02_Track_VertibotAttachPerk)
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer()
		Return
	EndIf
	SFL02VertibotPlayers.RemoveRef(playerRef)
	playerRef.RemovePerk(SFL02_Track_VertibotAttachPerk)
EndEvent
