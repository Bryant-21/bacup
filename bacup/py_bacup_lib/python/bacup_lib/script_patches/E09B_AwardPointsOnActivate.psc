Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef == Game.GetPlayer() && NWOT_Nukacade_Points != None && PointsToGive > 0
		playerRef.ModValue(NWOT_Nukacade_Points, PointsToGive)
		If NWOT_Nukacade_TutorialAV_PrizePoints != None && playerRef.GetValue(NWOT_Nukacade_TutorialAV_PrizePoints) <= 0.0
			playerRef.SetValue(NWOT_Nukacade_TutorialAV_PrizePoints, 1.0)
			If NWOT_Nukacade_Tutorial_PointsMSG != None
				NWOT_Nukacade_Tutorial_PointsMSG.Show()
			EndIf
		EndIf
	EndIf
EndEvent
