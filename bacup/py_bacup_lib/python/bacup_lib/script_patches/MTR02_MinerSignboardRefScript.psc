Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || MTR02_Miner == None || MTR02_Miner.IsCompleted()
		Return
	EndIf

	If !MTR02_Miner.IsRunning()
		MTR02_MinerMainQuestStartKeyword.SendStoryEventAndWait(akRef1 = akActionRef)
	EndIf

	If MTR02_Miner.IsRunning() && !MTR02_Miner.IsStageDone(10)
		MTR02_Miner.SetStage(10)
	EndIf
EndEvent
