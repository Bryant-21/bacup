GlobalVariable Function ResolveKeywordGlobal(Form subject, KeywordGlobalPairDatum[] pairs, KeywordGlobalPairDatum fallback)
	If subject && pairs != None
		Int index = 0
		While index < pairs.Length
			If pairs[index].FormKeyword && (subject == pairs[index].FormKeyword || subject.HasKeyword(pairs[index].FormKeyword))
				Return pairs[index].EnumGlobal
			EndIf
			index += 1
		EndWhile
	EndIf
	Return fallback.EnumGlobal
EndFunction

GlobalVariable Function ResolveQuestType(Form subject)
	Return ResolveKeywordGlobal(subject, QuestType_KeywordGlobalPairData, QuestType_Default_KeywordGlobalPair)
EndFunction

GlobalVariable Function ResolveQuestLocation(Form subject)
	Return ResolveKeywordGlobal(subject, QuestLocation_KeywordGlobalPairData, QuestLocation_Default_KeywordGlobalPair)
EndFunction

GlobalVariable Function ResolveQuestObject(Form subject)
	Return ResolveKeywordGlobal(subject, QuestObject_KeywordGlobalPairData, QuestObject_Default_KeywordGlobalPair)
EndFunction

GlobalVariable Function ResolveQuestTargetActor(Form subject)
	Return ResolveKeywordGlobal(subject, QuestTargetActor_KeywordGlobalPairData, QuestTargetActor_Default_KeywordGlobalPair)
EndFunction

GlobalVariable Function ResolveQuestEnemy(Form subject)
	Return ResolveKeywordGlobal(subject, QuestEnemy_KeywordGlobalPairData, QuestEnemy_Default_KeywordGlobalPair)
EndFunction
