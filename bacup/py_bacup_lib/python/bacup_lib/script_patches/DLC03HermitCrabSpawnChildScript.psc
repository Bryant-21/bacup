Event OnLoad()
	FindMyMommy()
EndEvent

Function FindMyMommy()
	ObjectReference[] nearbyParents = FindAllReferencesWithKeyword(DLC03_ActorTypeHermitCrab, 5000.0)
	Int parentIndex = 0
	Bool foundParent = False

	While parentIndex < nearbyParents.Length && !foundParent
		dlc03hermitcrabspawnscript candidateParent = nearbyParents[parentIndex] as dlc03hermitcrabspawnscript
		If candidateParent != None && candidateParent.FindParent(Self)
			myParent = candidateParent
			foundParent = True
		Else
			parentIndex += 1
		EndIf
	EndWhile
EndFunction

Event OnDeath(Actor akKiller)
	If Explosion_Storm_E01_ThunderCrabSpawnAoE != None
		PlaceAtMe(Explosion_Storm_E01_ThunderCrabSpawnAoE)
	EndIf
	DeleteWhenAble()
EndEvent

Event OnUnload()
	DeleteWhenAble()
EndEvent
