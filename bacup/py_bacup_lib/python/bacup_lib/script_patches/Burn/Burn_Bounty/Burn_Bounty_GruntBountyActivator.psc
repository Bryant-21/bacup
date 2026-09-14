Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef
		Return
	EndIf

	If GruntHuntQuest != None && (GruntHuntQuest.IsRunning() || playerRef.HasKeyword(GruntHuntActiveKeyword))
		If GruntHuntActiveMessage != None
			GruntHuntActiveMessage.Show()
		EndIf
		Return
	EndIf

	Burn:Burn_Bounty:Burn_Bounty_BountyGiverScript bountyMaster = BountyMasterNPCQuest as Burn:Burn_Bounty:Burn_Bounty_BountyGiverScript
	If bountyMaster != None
		bountyMaster.StartLocalGruntHunt(playerRef)
		If GruntHuntQuest != None && GruntHuntQuest.IsRunning() && GruntHuntStartedMessage != None
			GruntHuntStartedMessage.Show()
		EndIf
	EndIf
EndEvent
