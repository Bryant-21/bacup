; Nuclear Winter (battle royale) QA-bot harness. Fallout 4 has none of the FO76 APIs it
; drives: Actor.IsInsideStormZone(radius), Actor.GetHeadingToStormZoneCenter(),
; Actor.FindRandomReferenceWithKeyword(keyword, radius, bool), and the
; OnShowSpawnPickerEvent engine event.
; BEHAVIOUR LOST: storm-zone navigation (the bot no longer walks toward the shrinking
; ring), keyword-based loot-bag seeking, and the spawn-picker hook.
; @drop-member OnShowSpawnPickerEvent

Function MoveTowardsCenter(Bool moveLeft)
EndFunction

Event onTimer(Int aiTimerID)
	If aiTimerID == FireTimerID
		Self.StartFireTimer()
	ElseIf aiTimerID == LootTimerID
		Self.StartLootTimer()
	EndIf
EndEvent
