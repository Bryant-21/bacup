Bool Function StartLocalGruntHunt(Actor akPlayer)
	If akPlayer == None || GruntBountyQuest == None || GruntBountyStartKeyword == None
		Return False
	EndIf
	If GruntBountyQuest.IsRunning() || BountyLocationGroups == None || BountyLocationGroups.Length == 0
		Return False
	EndIf

	Int startIndex = 0
	If GruntLocationAVs != None && GruntLocationAVs.Length > 0 && GruntLocationAVs[0].GroupIndexAV != None
		startIndex = (akPlayer.GetValue(GruntLocationAVs[0].GroupIndexAV) as Int) + 1
	EndIf
	If startIndex < 0 || startIndex >= BountyLocationGroups.Length
		startIndex = 0
	EndIf

	Int checkedCount = 0
	Int groupIndex = startIndex
	Location chosenLocation = None
	While checkedCount < BountyLocationGroups.Length && chosenLocation == None
		BountyLocationGroup candidate = BountyLocationGroups[groupIndex]
		If candidate.IsGruntHuntLoc
			chosenLocation = candidate.HuntLocation01
		EndIf
		groupIndex += 1
		If groupIndex >= BountyLocationGroups.Length
			groupIndex = 0
		EndIf
		checkedCount += 1
	EndWhile

	If chosenLocation == None
		Return False
	EndIf

	Int chosenGroupIndex = groupIndex - 1
	If chosenGroupIndex < 0
		chosenGroupIndex = BountyLocationGroups.Length - 1
	EndIf
	RememberLocalGruntLocation(akPlayer, chosenGroupIndex)
	Return GruntBountyStartKeyword.SendStoryEventAndWait(chosenLocation, akPlayer, akPlayer)
EndFunction

Function RememberLocalGruntLocation(Actor akPlayer, Int aiGroupIndex)
	If akPlayer == None || GruntLocationAVs == None || GruntLocationAVs.Length == 0
		Return
	EndIf

	Int index = GruntLocationAVs.Length - 1
	While index > 0
		If GruntLocationAVs[index].GroupIndexAV != None && GruntLocationAVs[index - 1].GroupIndexAV != None
			akPlayer.SetValue(GruntLocationAVs[index].GroupIndexAV, akPlayer.GetValue(GruntLocationAVs[index - 1].GroupIndexAV))
		EndIf
		If GruntLocationAVs[index].LocationIndexAV != None && GruntLocationAVs[index - 1].LocationIndexAV != None
			akPlayer.SetValue(GruntLocationAVs[index].LocationIndexAV, akPlayer.GetValue(GruntLocationAVs[index - 1].LocationIndexAV))
		EndIf
		index -= 1
	EndWhile

	If GruntLocationAVs[0].GroupIndexAV != None
		akPlayer.SetValue(GruntLocationAVs[0].GroupIndexAV, aiGroupIndex as Float)
	EndIf
	If GruntLocationAVs[0].LocationIndexAV != None
		akPlayer.SetValue(GruntLocationAVs[0].LocationIndexAV, 0.0)
	EndIf
EndFunction
