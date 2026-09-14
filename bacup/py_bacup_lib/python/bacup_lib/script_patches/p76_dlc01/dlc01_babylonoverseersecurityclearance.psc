Bool Function TryActivate(ObjectReference akActionRef)
	If RequiredClearanceLevel < 1.0
		Return True
	EndIf
	Actor thePlayer = akActionRef as Actor
	If thePlayer
		Float totalWins = thePlayer.GetValue(Babylon_TotalWins)
		If totalWins >= RequiredClearanceLevel
			Self.Activate(akActionRef, True)
			Return True
		Else
			; FO76 Actor.ShowInsufficientOverseerRankMessage(required, current) drove a
			; Nuclear-Winter-specific UI widget with no Fallout 4 equivalent. The bound
			; RequiredClearanceMessage is shown instead, which preserves the player-facing
			; refusal feedback but not the rank numbers baked into the FO76 widget.
			If RequiredClearanceMessage != None
				RequiredClearanceMessage.Show()
			EndIf
		EndIf
	EndIf
	Return False
EndFunction
