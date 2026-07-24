Float Function GetWaitTimeForDistance()
	Actor localPlayer
	Float xDiff
	Float yDiff
	Float xyDistance
	Float zDistance
	Float distanceFactor
	Float ratio
	Float waitTime
	localPlayer = Game.GetPlayer()
	If localPlayer == None
		Return SoundDelayMax
	EndIf
	xDiff = Self.GetPositionX() - localPlayer.GetPositionX()
	yDiff = Self.GetPositionY() - localPlayer.GetPositionY()
	xyDistance = Math.Sqrt(xDiff * xDiff + yDiff * yDiff)
	zDistance = Math.Abs(Self.GetPositionZ() - localPlayer.GetPositionZ())
	distanceFactor = xyDistance + zDistance * ZWeight
	ratio = Math.Max(distanceFactor - MinDistance, 0.0) / Math.Max(MaxDistance - MinDistance, 1.0)
	waitTime = (SoundDelayMax - SoundDelayMin) * ratio + SoundDelayMin
	Return waitTime
EndFunction

Event OnLoad()
	If !Self.IsDisabled()
		LastSoundTimestamp = 0.0
		LastDelay = TimerFrequency
		Self.StartTimer(TimerFrequency, iPlaySoundTimerID)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	Actor localPlayer
	Float xDiff
	Float yDiff
	Float xyDistance
	Float zDistance
	Float distanceFactor
	Float ratio
	Float currentDelay
	Float remainingWaitTime
	If !Self.IsDisabled()
		localPlayer = Game.GetPlayer()
		If localPlayer == None
			LastDelay = TimerFrequency
			Self.StartTimer(LastDelay, iPlaySoundTimerID)
			Return
		EndIf
		xDiff = Self.GetPositionX() - localPlayer.GetPositionX()
		yDiff = Self.GetPositionY() - localPlayer.GetPositionY()
		xyDistance = Math.Sqrt(xDiff * xDiff + yDiff * yDiff)
		zDistance = Math.Abs(Self.GetPositionZ() - localPlayer.GetPositionZ())
		distanceFactor = xyDistance + zDistance * ZWeight
		ratio = Math.Max(distanceFactor - MinDistance, 0.0) / Math.Max(MaxDistance - MinDistance, 1.0)
		currentDelay = (SoundDelayMax - SoundDelayMin) * ratio + SoundDelayMin
		remainingWaitTime = Math.Min(LastSoundTimestamp - LastDelay, currentDelay)
		If remainingWaitTime <= 0.0
			If RequiredPerk == None || localPlayer.HasPerk(RequiredPerk)
				SoundToPlay.Play(Self as ObjectReference)
			EndIf
			remainingWaitTime = currentDelay
		EndIf
		LastSoundTimestamp = remainingWaitTime
		LastDelay = Math.Min(TimerFrequency, Math.Max(remainingWaitTime, 0.1))
		Self.StartTimer(LastDelay, iPlaySoundTimerID)
	EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() as ObjectReference
		Self.CancelTimer(iPlaySoundTimerID)
	EndIf
EndEvent
