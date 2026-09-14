State startstate
	Event OnTriggerEnter(ObjectReference akActionRef)
		If akActionRef == Game.GetPlayer() && myExplosion != None
			GoToState("donestate")
			Int explosionIndex = 0
			While explosionIndex < numExplosions
				PlaceAtMe(myExplosion)
				explosionIndex += 1
				If explosionIndex < numExplosions
					Utility.Wait(Utility.RandomFloat(minTimeBetweenExplosions, maxTimeBetweenExposions))
				EndIf
			EndWhile
		EndIf
	EndEvent
EndState
