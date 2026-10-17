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

; FO76 decided passage per client on the server (ReevaluateConditions) and dropped the barrier
; collision for that client only. FO4 has no per-actor collision, so a grid the player may pass
; is disabled, the same way FO4 quests remove force fields and tripwire grids.
; Timer 1 re-checks a denied player standing at the grid; 300/400 are the approach/leave radii.

Event OnLoad()
	RequestReevaluateConditions()
EndEvent

Event OnUnload()
	CancelTimer(1)
	UnregisterForDistanceEvents(Game.GetPlayer(), Self)
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	RequestReevaluateConditions()
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	CancelTimer(1)
	RequestReevaluateConditions()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 1 && Is3DLoaded()
		RequestReevaluateConditions()
	EndIf
EndEvent

Function ReevaluateConditions(Actor akSendingPlayer)
	Actor playerRef = Game.GetPlayer()
	If akSendingPlayer == None
		akSendingPlayer = playerRef
	EndIf
	If !IsEnabled()
		CancelTimer(1)
		UnregisterForDistanceEvents(playerRef, Self)
		Return
	EndIf
	If PassesAccessConditions(akSendingPlayer)
		OpenForPlayer()
		Return
	EndIf

	Bool playerAtGrid = Is3DLoaded() && GetDistance(playerRef) <= 400.0
	UpdateClientAnimationStateValue(playerAtGrid, False)
	UnregisterForDistanceEvents(playerRef, Self)
	If playerAtGrid
		RegisterForDistanceGreaterThanEvent(playerRef, Self, 400.0)
		StartTimer(1.0, 1)
	ElseIf Is3DLoaded()
		RegisterForDistanceLessThanEvent(playerRef, Self, 300.0)
	EndIf
EndFunction

Function OpenForPlayer()
	CancelTimer(1)
	UnregisterForDistanceEvents(Game.GetPlayer(), Self)
	UpdateClientAnimationStateValue(True, True)
	Disable(True)
	If IsEnabled()
		; Script Disable() does not apply to an enable-parented ref (the Whitespring Bunker entrance
		; grids are). Drop the barrier out of reach so it stops blocking; a cell reset restores the
		; position and OnLoad re-evaluates.
		SetPosition(GetPositionX(), GetPositionY(), GetPositionZ() - 10000.0)
	EndIf
EndFunction

; RestrictedAccessScript's property contract, evaluated for one actor. The instance-owner checks
; always hold in single player.
Bool Function PassesAccessConditions(Actor akActor)
	If DisableAccessRestrictions && DisableAccessRestrictions.GetValue() > 0.0
		Return True
	EndIf
	If akActor == None
		Return False
	EndIf

	Int conditionCount = 0
	Int metCount = 0
	If RequiredKeyword
		conditionCount += 1
		If akActor.HasKeyword(RequiredKeyword)
			metCount += 1
		EndIf
	EndIf
	If RequiredObjectKeyword
		conditionCount += 1
		If HasKeyword(RequiredObjectKeyword)
			metCount += 1
		EndIf
	EndIf
	If RequiredFaction
		conditionCount += 1
		If akActor.IsInFaction(RequiredFaction)
			metCount += 1
		EndIf
	EndIf
	If RequiredActorValue
		conditionCount += 1
		If akActor.GetValue(RequiredActorValue) >= RequiredActorValueValue as Float
			metCount += 1
		EndIf
	EndIf
	If RequiredItem
		conditionCount += 1
		If akActor.GetItemCount(RequiredItem) > 0
			metCount += 1
		EndIf
	EndIf
	If CompletedQuest
		conditionCount += 1
		If CompletedQuest.IsCompleted()
			metCount += 1
		EndIf
	EndIf
	If RequiredInstanceOwner
		conditionCount += 1
		metCount += 1
	EndIf

	If conditionCount == 0
		Return !RequiresAccessConditions
	EndIf
	If UseORConditions
		Return metCount > 0
	EndIf
	Return metCount == conditionCount
EndFunction

; Non-latent: PlayAnimationAndWait on "TransitionComplete" holds the shared lock forever if the
; converted graph never sends that event, which would stall every later re-evaluation.
Function UpdateClientAnimationState(Int laserGridNextState)
	If !Is3DLoaded() || laserGridCurrentState == laserGridNextState
		Return
	EndIf
	laserGridCurrentState = laserGridNextState
	If laserGridNextState == 0
		PlayAnimation(Purple)
	ElseIf laserGridNextState == 1
		PlayAnimation(Blue)
	Else
		PlayAnimation(Red)
		If NuclearWinterMode
			ShowInsufficientOverseerRankMessage()
		EndIf
	EndIf
EndFunction
