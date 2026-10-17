Event OnLoad()
	If !AmbushFurniture || AmbushFurniture.Length == 0
		AmbushFurniture = Self.GetLinkedRefChain(LinkHabitatAmbush, 100)
		InitSpawnCount()
	EndIf
	If numberToSpawnRemaining > 0
		Self.RegisterForHitEvent(Self as ScriptObject, None, None, None, -1, -1, -1, -1, True)
	EndIf
EndEvent

Event OnReset()
	; A respawned habitat rebuilds its creature budget on the next load.
	AmbushFurniture = None
	numberToSpawnRemaining = 0
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	LastAttacker = akAggressor as Actor
	SpawnIntoFurniture(WaitTimeBeforeEachSpawnOnHit)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	SpawnIntoFurniture(WaitTimeBeforeEachSpawnOnDestructionStateChange)
EndEvent

Function InitSpawnCount()
	Int furnitureCount = 0
	If AmbushFurniture
		furnitureCount = AmbushFurniture.Length
	EndIf
	Int maxCount = furnitureCount
	If MaxCreatures >= 0 && MaxCreatures < maxCount
		maxCount = MaxCreatures
	EndIf
	Int minCount = MinCreatures
	If minCount > maxCount
		minCount = maxCount
	EndIf
	numberToSpawnRemaining = Utility.RandomInt(minCount, maxCount)
EndFunction

Function SpawnIntoFurniture(Float afWait)
	If !AmbushFurniture || !ActorToSpawn
		Return
	EndIf
	Int i = 0
	While i < AmbushFurniture.Length && numberToSpawnRemaining > 0
		If AmbushFurniture[i]
			SpawnCreature(AmbushFurniture[i], afWait)
		EndIf
		i += 1
	EndWhile
EndFunction
