Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || MessageToShow == None
		Return
	EndIf
	Int button = MessageToShow.Show()
	Int index = 0
	While index < MessageChallengeData.Length
		MessageChallengeDatum datum = MessageChallengeData[index]
		If datum.ButtonToCheck == button
			Bool passed = True
			If datum.Item_ToCheck != None
				passed = playerRef.GetItemCount(datum.Item_ToCheck) >= datum.ItemCountNeeded
			ElseIf datum.SPECIAL_ToCheck != None && datum.SPECIAL_Challenge_Global != None
				passed = playerRef.GetValue(datum.SPECIAL_ToCheck) >= datum.SPECIAL_Challenge_Global.GetValue()
			EndIf
			If passed
				If datum.PassMessage != None
					datum.PassMessage.Show()
				EndIf
				If SoundToPlay != None
					SoundToPlay.Play(Self)
				EndIf
				ObjectReference linkedRef = GetLinkedRef()
				If linkedRef != None
					If PassAction == 0
						linkedRef.Disable()
					ElseIf PassAction == 1
						linkedRef.Enable()
					ElseIf PassAction == 2
						linkedRef.Activate(playerRef)
					ElseIf PassAction == 3
						linkedRef.Lock(False)
					EndIf
				EndIf
				If DisableSelf
					Disable()
				EndIf
			ElseIf datum.FailMessage != None
				datum.FailMessage.Show()
			EndIf
			Return
		EndIf
		index += 1
	EndWhile
EndEvent
