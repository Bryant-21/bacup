Function RequestReevaluateConditions()
	; FO76 SendRMIToServer("ReevaluateConditions") -> direct local call.
	Self.ReevaluateConditions(Game.GetPlayer())
EndFunction

Function ShowInsufficientOverseerRankMessage()
	; FO76 Actor.ShowInsufficientOverseerRankMessage(required, current) drove a
	; Nuclear-Winter-specific UI widget with no Fallout 4 equivalent. BEHAVIOUR LOST:
	; the on-screen "insufficient Overseer rank" popup. The audio cue is kept.
	Actor thePlayer = game.GetPlayer()
	If thePlayer && BabylonUIOverseerLevelLow
		BabylonUIOverseerLevelLow.Play(Self as objectreference)
	EndIf
EndFunction
