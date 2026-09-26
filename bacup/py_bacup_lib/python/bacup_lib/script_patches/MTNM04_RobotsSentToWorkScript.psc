MTNM04QuestScript Function QuestScript()
	If myQIScript == None
		myQI = GetOwningQuest()
		myQIScript = myQI as MTNM04QuestScript
	EndIf
	Return myQIScript
EndFunction

Function ExplodeRobot(Actor akRobot)
	If akRobot == None
		Return
	EndIf
	If crExplosionRobotSelfDestruct != None
		akRobot.PlaceAtMe(crExplosionRobotSelfDestruct)
	EndIf
	If c_Steel_scrap != None
		akRobot.PlaceAtMe(c_Steel_scrap, 2)
	EndIf
EndFunction

Event OnEnterBleedout(ObjectReference akSenderRef)
	MTNM04QuestScript questScript = QuestScript()
	If questScript != None
		questScript.RobotEnteredBleedout(akSenderRef as Actor)
	EndIf
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	MTNM04QuestScript questScript = QuestScript()
	If questScript != None
		questScript.RepairRobot(akSenderRef as Actor, akActionRef as Actor)
	EndIf
EndEvent
