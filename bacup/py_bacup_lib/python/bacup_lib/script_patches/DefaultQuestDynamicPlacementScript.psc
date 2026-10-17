Event OnQuestInit()
	; Random encounters create their props at the center marker; this lays each one out at its
	; authored offset (local offsets follow the marker's heading) and rotation.
	ObjectReference center = CenterMarker.GetReference()
	If !center || !Objects
		Return
	EndIf
	Float heading = center.GetAngleZ()
	Int i = 0
	While i < Objects.Length
		ObjectData row = Objects[i]
		If row
			PlaceObject(row, center, heading)
		EndIf
		i += 1
	EndWhile
EndEvent

Function PlaceObject(ObjectData row, ObjectReference center, Float heading)
	ObjectReference target = None
	If row.Object
		target = row.Object.GetReference()
	EndIf
	If !target && row.ObjectRef
		target = row.ObjectRef
	EndIf
	If !target && row.BaseObject
		target = center.PlaceAtMe(row.BaseObject, 1, False, False, row.bBaseObjectCreateTemp)
		If target && row.Object
			row.Object.ForceRefTo(target)
		EndIf
	EndIf
	If !target
		Return
	EndIf
	Float x = row.XPosOffset
	Float y = row.YPosOffset
	Float z = row.ZPosOffset
	If row.bIsRandomOffset
		x = Utility.RandomFloat(-x, x)
		y = Utility.RandomFloat(-y, y)
	EndIf
	If row.bIsLocalOffset
		Float localX = x
		x = localX * Math.Cos(heading) + y * Math.Sin(heading)
		y = y * Math.Cos(heading) - localX * Math.Sin(heading)
	EndIf
	target.MoveTo(center, x, y, z, False)
	target.SetAngle(center.GetAngleX() + row.XRotOffset, center.GetAngleY() + row.YRotOffset, heading + row.ZRotOffset)
EndFunction
