; @extends ReferenceAlias
; FO76 put this on the player's NPC_ record, which FO4 cannot carry. Tales'
; B21_TFA_qust_PlayerScripts holds it on a PlayerRef alias instead.
; The client body survived; these members only move it from Self to the alias reference and restore the
; event lookup the decompiler dropped (ARRAY_FINDELEMENT). In single player the shooter is the player, so
; FO76's nearby-player distance test always passed.
Event OnAliasInit()
	RegisterForShootingEvent()
EndEvent

Event OnLoad()
	RegisterForShootingEvent()
EndEvent

; A save taken while ducked must not keep the category muted.
Event OnPlayerLoadGame()
	If SoundCategoryToDuckAfterShot != None
		SoundCategoryToDuckAfterShot.UnMute()
	EndIf
	RegisterForShootingEvent()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == ShotDuckingTimerID && SoundCategoryToDuckAfterShot != None
		SoundCategoryToDuckAfterShot.UnMute()
	EndIf
EndEvent

Bool Function CheckForIgnoredWeapons()
	Actor playerRef = GetActorReference()
	If playerRef == None || KeywordsForIgnoredWeapons == None
		Return False
	EndIf
	Weapon equipped = playerRef.GetEquippedWeapon(0)
	If equipped == None
		Return False
	EndIf
	Int index = 0
	While index < KeywordsForIgnoredWeapons.Length
		If KeywordsForIgnoredWeapons[index] != None && equipped.HasKeyword(KeywordsForIgnoredWeapons[index])
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function RegisterForShootingEvent()
	ObjectReference playerRef = GetReference()
	If playerRef == None
		Return
	EndIf
	Int index = 0
	While EventsToListenFor != None && index < EventsToListenFor.Length
		RegisterForAnimationEvent(playerRef, EventsToListenFor[index])
		index += 1
	EndWhile
	RegisterForAnimationEvent(playerRef, "weaponAttach")
EndFunction

State ListeningForEvent
	Event OnAnimationEvent(ObjectReference akSource, String asEventName)
		GoToState("busy")
		If asEventName == "weaponAttach"
			Utility.Wait(0.5)
		ElseIf SoundCategoryToDuckAfterShot != None && EventsToListenFor != None && EventsToListenFor.Find(asEventName) >= 0 && !CheckForIgnoredWeapons()
			SoundCategoryToDuckAfterShot.Mute()
			StartTimer(Utility.RandomFloat(MinSecondsToDuckSoundCateogryAfterShot, MaxSecondsToDuckSoundCategoryAfterShot), ShotDuckingTimerID)
		EndIf
		GoToState("ListeningForEvent")
	EndEvent
EndState
