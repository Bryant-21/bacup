MTNS04QuestScript Function EventScript()
	If ActiveInstance == None
		ActiveInstance = GetOwningQuest()
	EndIf
	Return ActiveInstance as MTNS04QuestScript
EndFunction

Function WatchInstrumentHits()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MTNS04_InstrumentsList != None
		RegisterForHitEvent(Self, playerRef, MTNS04_InstrumentsList)
	EndIf
EndFunction

Event OnAliasInit()
	ActiveInstance = GetOwningQuest()
	If HallTrigger != None
		myTrigger = HallTrigger.GetReference()
	EndIf
	WatchInstrumentHits()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	MTNS04QuestScript eventScript = EventScript()
	If eventScript != None && eventScript.IsRunning()
		eventScript.AddWeaponAggro(akAggressor as Actor, akSource as Weapon, eventScript.iAggroWeaponHitAmount)
		WatchInstrumentHits()
	EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
	MTNS04QuestScript eventScript = EventScript()
	If eventScript == None || akKiller == None || akKiller != Game.GetPlayer()
		Return
	EndIf
	; Only kills scored with the beer-hall instrument weapons feed the noise meter.
	eventScript.AddWeaponAggro(akKiller, akKiller.GetEquippedWeapon(), eventScript.iDeathAggroAmount)
EndEvent

Event OnAliasShutdown()
	UnregisterForAllHitEvents()
EndEvent
