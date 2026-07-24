Event OnEffectStart(Actor akTarget, Actor akCaster)
	targetRef = akTarget
	If !targetRef || !CreatureData || !VariantKeyword || !TransitionShaderVFX || !VariantQuest
		Return
	EndIf

	If targetRef.HasKeyword(VariantKeyword)
		Return
	EndIf

	currentCreatureData.CreatureRace = None
	race targetRace = targetRef.GetRace()
	Int i = 0
	While i < CreatureData.Length
		If CreatureData[i].CreatureRace == targetRace
			currentCreatureData = CreatureData[i]
			i = CreatureData.Length
		Else
			i += 1
		EndIf
	EndWhile

	If !currentCreatureData.CreatureRace
		Return
	EndIf

	factionsToRemove = new faction[4]
	Int n = 0
	If currentCreatureData.OriginalFaction01
		factionsToRemove[n] = currentCreatureData.OriginalFaction01
		n += 1
	EndIf
	If currentCreatureData.OriginalFaction02
		factionsToRemove[n] = currentCreatureData.OriginalFaction02
		n += 1
	EndIf
	If currentCreatureData.OriginalFaction03
		factionsToRemove[n] = currentCreatureData.OriginalFaction03
		n += 1
	EndIf
	If currentCreatureData.OriginalFaction04
		factionsToRemove[n] = currentCreatureData.OriginalFaction04
		n += 1
	EndIf

	Int j = 0
	While j < n
		If targetRef.IsInFaction(factionsToRemove[j])
			targetRef.RemoveFromFaction(factionsToRemove[j])
		EndIf
		j += 1
	EndWhile

	If VariantFaction
		targetRef.AddToFaction(VariantFaction)
	EndIf

	If currentCreatureData.VariantMod
		targetRef.AttachMod(currentCreatureData.VariantMod)
	EndIf

	targetRef.AddKeyword(VariantKeyword)
	TransitionShaderVFX.Cast(targetRef, targetRef)

	If OnBecomeVariantExplosion
		targetRef.PlaceAtMe(OnBecomeVariantExplosion)
	EndIf

	RefCollectionAlias nameCollection = VariantQuest.GetAlias(1) as RefCollectionAlias
	If nameCollection
		nameCollection.AddRef(targetRef)
	EndIf
EndEvent
