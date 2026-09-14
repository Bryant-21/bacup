Coord Function GetOverflowSpiralPlacement(Int clientIndex, String[] cells)
	; FO76 Game.GetFormByEditorID(string) and Cell.GetWorldXY() have no Fallout 4
	; equivalent -- FO4 cannot resolve a form from an EditorID at runtime, nor read a
	; cell grid coordinate. BEHAVIOUR LOST: the preset-cell exclusion list, so the
	; spiral placement no longer avoids the authored cells.
	Coord[] presetCoords = new Coord[0]
	Int j = 1
	Int validCells = 0
	Coord spiralCoord
	While j < 81
		spiralCoord = Self.GetSpiralCoords(j)
		spiralCoord = Self.CreateCoord(spiralCoord.x * 5, spiralCoord.y * 5)
		If !Self.IsNearCoordList(spiralCoord, presetCoords)
			validCells = validCells + 1
			If validCells > clientIndex
				Return Self.CreateCoord(spiralCoord.x, spiralCoord.y)
			EndIf
		EndIf
		j = j + 1
	EndWhile
	Return None
EndFunction
