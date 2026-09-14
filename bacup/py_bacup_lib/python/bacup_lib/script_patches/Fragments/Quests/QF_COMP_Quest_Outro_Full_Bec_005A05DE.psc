Function Fragment_Stage_0001_Item_00()
	GiveGarageKey()
	DisplayObjectiveOnce(100)
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	GiveGarageKey()
	DisplayObjectiveOnce(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
	ObjectReference watogaDoor = Alias_WUDoor.GetReference()
	If watogaDoor
		watogaDoor.Unlock()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	CompleteObjectiveOnce(100)
	DisplayObjectiveOnce(200)
	EvaluateAlias(Alias_Beckett)
EndFunction

Function Fragment_Stage_0300_Item_00()
	CompleteObjectiveOnce(200)
	DisplayObjectiveOnce(300)
	StartSceneOnce(COMP_Quest_Outro_Full_Beckett_RonnyLeaves)
EndFunction

Function Fragment_Stage_0350_Item_00()
	EvaluateAlias(Alias_Ronny)
EndFunction

Function Fragment_Stage_0400_Item_00()
	CompleteObjectiveOnce(300)
	DisplayObjectiveOnce(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
	CompleteObjectiveOnce(400)
	ObjectReference watogaDoor = Alias_WUDoor.GetReference()
	If watogaDoor
		watogaDoor.Unlock()
		watogaDoor.SetOpen(True)
	EndIf
	PopulateRaiderAllies()
	DisplayObjectiveOnce(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
	CompleteObjectiveOnce(500)
	DisplayObjectiveOnce(600)
EndFunction

Function Fragment_Stage_0650_Item_00()
	Actor beckett = Alias_Beckett.GetActorReference()
	ObjectReference sceneMarker = Alias_FrankySceneTPMarker.GetReference()
	If beckett && sceneMarker
		beckett.MoveTo(sceneMarker)
		beckett.EvaluatePackage()
	EndIf
	ObjectReference frankieDoor = Alias_FrankieDoor.GetReference()
	If frankieDoor
		frankieDoor.Unlock()
		frankieDoor.SetOpen(True)
	EndIf
	StartSceneOnce(pCOMP_Quest_Outro_Full_Beckett_Confrontation)
EndFunction

Function Fragment_Stage_0700_Item_00()
	CompleteObjectiveOnce(600)
	DisplayObjectiveOnce(700)
	DisplayObjectiveOnce(710)
EndFunction

Function Fragment_Stage_0705_Item_00()
	CompleteObjectiveOnce(700)
	StartSceneOnce(pCOMP_Quest_Outro_Full_Beckett_KillFrankie)
EndFunction

Function Fragment_Stage_0710_Item_00()
	CompleteObjectiveOnce(710)
	StartSceneOnce(COMP_Quest_Outro_Full_Beckett_SaveFrankie)
EndFunction

Function Fragment_Stage_0720_Item_00()
	SetFrankieDiesValue(True)
	StartSceneOnce(pCOMP_Quest_Outro_Full_Beckett_FrankieDead)
EndFunction

Function Fragment_Stage_0750_Item_00()
	SetFrankieDiesValue(False)
	StartSceneOnce(pCOMP_Quest_Outro_Full_Beckett_FrankieLives)
EndFunction

Function Fragment_Stage_0755_Item_00()
	Actor frankie = Alias_FrankieClaw.GetActorReference()
	If frankie
		frankie.GetActorBase().SetProtected(False)
	EndIf
EndFunction

Function Fragment_Stage_0760_Item_00()
	SetFrankieDiesValue(True)
EndFunction

Function Fragment_Stage_0800_Item_00()
	CompleteObjectiveOnce(700)
	CompleteObjectiveOnce(710)
	DisplayObjectiveOnce(800)
	EvaluateRaiderAllies()
EndFunction

Function Fragment_Stage_0900_Item_00()
	CompleteObjectiveOnce(800)
	If pCOMP_Quest_Camp_Full_Beckett && pCOMP_Quest_Camp_Full_Beckett.IsRunning() && !pCOMP_Quest_Camp_Full_Beckett.IsStageDone(9000)
		pCOMP_Quest_Camp_Full_Beckett.SetStage(9000)
	EndIf
	If pCOMP_Quest_Camp_Full_Beckett
		ReferenceAlias campBeckettAlias = pCOMP_Quest_Camp_Full_Beckett.GetAlias(1) as ReferenceAlias
		If campBeckettAlias
			Actor campBeckett = campBeckettAlias.GetActorReference()
			If Alias_BeckettAtCAMP.GetReference() == None && campBeckett
				Alias_BeckettAtCAMP.ForceRefIfEmpty(campBeckett)
			EndIf
		EndIf
	EndIf
	EvaluateAlias(Alias_BeckettAtCAMP)
	DisplayObjectiveOnce(900)
EndFunction

Function Fragment_Stage_1000_Item_00()
	CompleteObjectiveOnce(900)
	Actor player = GetPlayerActor()
	If player && pCOMP_AV_Beckett_FinaleComplete
		player.SetValue(pCOMP_AV_Beckett_FinaleComplete, 1.0)
	EndIf
	If pCOMP_Quest_Camp_Full_Beckett && pCOMP_Quest_Camp_Full_Beckett.IsRunning() && !pCOMP_Quest_Camp_Full_Beckett.IsStageDone(9100)
		pCOMP_Quest_Camp_Full_Beckett.SetStage(9100)
	EndIf
	If pCOMP_Quest_Camp_Full_Beckett && pCOMP_Quest_Camp_Full_Beckett.IsRunning() && !pCOMP_Quest_Camp_Full_Beckett.IsStageDone(9999)
		pCOMP_Quest_Camp_Full_Beckett.SetStage(9999)
	EndIf
	CompleteQuest()
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Actor player = GetPlayerActor()
	If player && pWU_GarageAKey
		Int keyCount = player.GetItemCount(pWU_GarageAKey)
		If keyCount > 0
			player.RemoveItem(pWU_GarageAKey, keyCount, True)
		EndIf
	EndIf
	DisableRaiderAllies()
	If !IsStopped()
		Stop()
	EndIf
EndFunction

Actor Function GetPlayerActor()
	Actor player = Alias_Player.GetActorReference()
	If !player
		player = Game.GetPlayer()
		If player
			Alias_Player.ForceRefIfEmpty(player)
		EndIf
	EndIf
	Return player
EndFunction

Function GiveGarageKey()
	Actor player = GetPlayerActor()
	If player && pWU_GarageAKey && player.GetItemCount(pWU_GarageAKey) == 0
		player.AddItem(pWU_GarageAKey, 1, True)
	EndIf
EndFunction

Function DisplayObjectiveOnce(Int objective)
	If !IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
		SetObjectiveDisplayed(objective)
	EndIf
EndFunction

Function CompleteObjectiveOnce(Int objective)
	If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
		SetObjectiveCompleted(objective)
	EndIf
EndFunction

Function StartSceneOnce(Scene targetScene)
	If targetScene && !targetScene.IsPlaying()
		targetScene.Start()
	EndIf
EndFunction

Function EvaluateAlias(ReferenceAlias targetAlias)
	Actor target = targetAlias.GetActorReference()
	If target
		target.EvaluatePackage()
	EndIf
EndFunction

Function PopulateRaiderAllies()
	If !pWL026_LvlRaider || Alias_RaiderAllies.GetCount() > 0
		Return
	EndIf
	Int index = 0
	While index < Alias_RaiderAlliesSpawn.GetCount()
		ObjectReference marker = Alias_RaiderAlliesSpawn.GetAt(index)
		If marker
			Actor ally = marker.PlaceAtMe(pWL026_LvlRaider, 1, True, False, False) as Actor
			If ally
				Alias_RaiderAllies.AddRef(ally)
				ally.EvaluatePackage()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function EvaluateRaiderAllies()
	Int index = 0
	While index < Alias_RaiderAllies.GetCount()
		Actor ally = Alias_RaiderAllies.GetAt(index) as Actor
		If ally
			ally.EvaluatePackage()
		EndIf
		index += 1
	EndWhile
EndFunction

Function DisableRaiderAllies()
	Int index = 0
	While index < Alias_RaiderAllies.GetCount()
		ObjectReference ally = Alias_RaiderAllies.GetAt(index)
		If ally && !ally.IsDisabled()
			ally.Disable()
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetFrankieDiesValue(Bool frankieDies)
	Actor player = GetPlayerActor()
	If player && COMP_AV_Beckett_FrankieDies
		If frankieDies
			player.SetValue(COMP_AV_Beckett_FrankieDies, 1.0)
		Else
			player.SetValue(COMP_AV_Beckett_FrankieDies, 0.0)
		EndIf
	EndIf
EndFunction
