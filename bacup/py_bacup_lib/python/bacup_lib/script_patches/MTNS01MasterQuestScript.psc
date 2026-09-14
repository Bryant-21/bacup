Bool Function MTNS01_SendQuestEvent(Keyword akStartKeyword)
    Actor playerRef = Game.GetPlayer()
    If akStartKeyword == None || playerRef == None
        Return False
    EndIf
    Return akStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
EndFunction

Bool Function MTNS01_StartIntro()
    Return MTNS01_SendQuestEvent(MTNS01_Intro_Quest_Keyword)
EndFunction

Bool Function MTNS01_StartMayhem()
    Return MTNS01_SendQuestEvent(MTNM01_Mayhem_Quest_Keyword)
EndFunction

Bool Function MTNS01_StartRaiders()
    Return MTNS01_SendQuestEvent(MTNL01_Raiders_Quest_Keyword)
EndFunction

Bool Function MTNS01_StartMissingLink()
    Return MTNS01_SendQuestEvent(MTN_MQ_Missing_Quest_Keyword)
EndFunction
