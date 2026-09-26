Event OnQuestInit()
    B21PointsGranted = False
EndEvent

; Nuka-Cade points are paid once per successful run; the spell's effect adds the points.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If B21PointsGranted || !IsCompleted()
        Return
    EndIf
    B21PointsGranted = True
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || NWOT_Nukacade_RewardPointsSpell == None
        Return
    EndIf
    If eventQuest != None && !eventQuest.IsPlayerParticipating()
        Return
    EndIf
    NWOT_Nukacade_RewardPointsSpell.Cast(playerRef, playerRef)
EndEvent
